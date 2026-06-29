#!/usr/bin/env python3
"""Download Amazon Chronos-Bolt-Tiny for offline forecasting."""

import sys
import os

# FIX 1: Match model to tiny variant for fast CPU inference
MODEL_ID = os.getenv("KRONOS_MODEL_PATH", "amazon/chronos-bolt-tiny")

def main():
    try:
        import torch
        import numpy as np
        from chronos import ChronosBoltPipeline
    except ImportError as e:
        print(f"Missing dependency: {e}")
        print("Run: pip3 install chronos-forecasting")
        sys.exit(1)

    print(f"[Download] Fetching {MODEL_ID}...")

    try:
        pipeline = ChronosBoltPipeline.from_pretrained(
            MODEL_ID,
            device_map="cpu",
            torch_dtype=torch.float32,
        )
    except Exception as e:
        print(f"Download failed: {e}")
        sys.exit(1)

    # FIX 3: Verify true median reduction works
    print("[Download] Running smoke test...")
    context = torch.tensor([100.0, 101.0, 102.0, 101.5, 103.0], dtype=torch.float32)
    forecast = pipeline.predict(context=context.unsqueeze(0), prediction_length=3)

    raw_samples = forecast[0].numpy()
    median_trajectory = np.median(raw_samples, axis=0)

    print(f"[Download] Smoke test passed.")
    print(f"[Download] Input:  [100, 101, 102, 101.5, 103]")
    print(f"[Download] Median: {[f'{c:.2f}' for c in median_trajectory]}")

    cache_dir = os.path.join(os.path.expanduser("~"), ".cache", "huggingface", "hub")
    print(f"[Download] Model cached at: {cache_dir}")
    print(f"[Download] Ready — start with: python3 main.py")

if __name__ == "__main__":
    main()
