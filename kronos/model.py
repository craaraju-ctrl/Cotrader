"""
Kronos Model — AI time-series forecasting powered by Amazon Chronos-Bolt.

FIX 1: Uses chronos-bolt-tiny (~28M params) for fast CPU inference (~100ms).
FIX 3: True median reduction via np.median(axis=0) instead of index-based sampling.
"""

import numpy as np
import pandas as pd
import torch
import warnings
import sys
import os

warnings.filterwarnings("ignore", category=FutureWarning)

_pipeline = None
_pipeline_error = None

# FIX 1: Default to chronos-bolt-tiny for fast inference
MODEL_ID = os.getenv("KRONOS_MODEL_PATH", "amazon/chronos-bolt-tiny")


def _load_pipeline():
    global _pipeline, _pipeline_error
    if _pipeline is not None:
        return _pipeline
    if _pipeline_error:
        return None

    try:
        from chronos import ChronosBoltPipeline
        print(f"[Kronos] Loading {MODEL_ID}...", file=sys.stderr)
        _pipeline = ChronosBoltPipeline.from_pretrained(
            MODEL_ID,
            device_map="cpu",
            torch_dtype=torch.float32,
        )
        print(f"[Kronos] {MODEL_ID} loaded — real AI forecasting active", file=sys.stderr)
        return _pipeline
    except Exception as e:
        _pipeline_error = str(e)
        print(f"[Kronos] Could not load {MODEL_ID}: {e}", file=sys.stderr)
        print("[Kronos] Falling back to drift model. Run: python3 download.py", file=sys.stderr)
        return None


def _drift_forecast(df, y_timestamp, pred_len):
    """Exponentially-weighted drift model with GARCH-like volatility."""
    closes = df["close"].values.astype(float)
    if len(closes) < 2:
        closes = np.array([closes[-1], closes[-1]])

    returns = np.diff(closes) / closes[:-1]
    weights = np.exp(np.linspace(-1, 0, len(returns)))
    weights /= weights.sum()
    drift = float(np.dot(weights, returns))
    sigma = float(np.std(returns) * max(0.5, 1.0 - abs(drift) * 20))

    preds = []
    current_close = float(closes[-1])
    last_volume = float(df["volume"].iloc[-1])

    for _ in range(pred_len):
        damped_drift = drift * (1.0 - abs(drift) * 5)
        noise = np.random.normal(0, sigma)
        ret = damped_drift + noise
        new_close = current_close * (1.0 + ret)
        new_open = current_close
        half_range = abs(new_close - new_open) + abs(np.random.normal(0, current_close * 0.001))
        preds.append({
            "open": new_open,
            "high": max(new_open, new_close) + half_range * 0.4,
            "low": min(new_open, new_close) - half_range * 0.4,
            "close": new_close,
            "volume": last_volume * abs(np.random.normal(1.0, 0.08)),
        })
        current_close = new_close

    return pd.DataFrame(preds, index=y_timestamp)


class KronosPredictor:
    def __init__(self):
        self.pipeline = _load_pipeline()
        self.using_real_model = self.pipeline is not None

    def predict(self, df, x_timestamp, y_timestamp, pred_len,
                temperature=0.8, top_p=0.9, sample_count=20):
        if self.using_real_model:
            return self._chronos_predict(df, y_timestamp, pred_len, sample_count)
        else:
            return _drift_forecast(df, y_timestamp, pred_len)

    def _chronos_predict(self, df, y_timestamp, pred_len, sample_count=20):
        """FIX 3: True median reduction across all sample paths."""
        close_prices = torch.tensor(df["close"].values, dtype=torch.float32)

        with torch.no_grad():
            forecast = self.pipeline.predict(
                context=close_prices.unsqueeze(0),
                prediction_length=pred_len,
            )

        # FIX 3: np.median(axis=0) computes true 50th percentile across all samples
        # forecast shape: [num_samples, pred_len]
        pred_tensor = forecast[0].numpy()
        closes_pred = np.median(pred_tensor, axis=0)

        preds = []
        last_close = float(df["close"].iloc[-1])
        last_volume = float(df["volume"].iloc[-1])

        for i, close in enumerate(closes_pred):
            close = float(close)
            open_ = last_close if i == 0 else float(closes_pred[i - 1])
            rng = abs(close - open_) + abs(np.random.normal(0, close * 0.0008))
            preds.append({
                "open": open_,
                "high": max(open_, close) + rng * 0.3,
                "low": min(open_, close) - rng * 0.3,
                "close": close,
                "volume": last_volume * abs(np.random.normal(1.0, 0.06)),
            })
            last_close = close

        print(
            f"[Kronos] Chronos-Bolt forecast: {len(preds)} bars | "
            f"Entry: {df['close'].iloc[-1]:.4f} -> Predicted: {closes_pred[-1]:.4f}",
            file=sys.stderr
        )
        return pd.DataFrame(preds, index=y_timestamp)
