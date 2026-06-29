from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
from typing import List
import pandas as pd
from model import KronosPredictor
import uvicorn
import os
import time

app = FastAPI(title="Kronos Forecasting Service for rat", version="0.1.0")

print("[KronosService] Loading Kronos model...")
predictor = KronosPredictor()
_start_time = time.time()
print("[KronosService] Kronos model loaded successfully and ready.")

class OhlcvBar(BaseModel):
    timestamp: str
    open: float
    high: float
    low: float
    close: float
    volume: float

class ForecastRequest(BaseModel):
    symbol: str
    ohlcv: List[OhlcvBar]
    timeframe: str = Field(default="1min", description="Pandas freq string: '1min', '5min', '1H', '1D'")
    pred_len: int = Field(default=10, ge=1, le=25)
    temperature: float = 0.8
    top_p: float = 0.9
    sample_count: int = 20

class ForecastResponse(BaseModel):
    symbol: str
    forecasts: List[dict]
    message: str

@app.get("/health")
async def health():
    uptime = int(time.time() - _start_time)
    return {
        "status": "ok",
        "model": "chronos-bolt-tiny",
        "version": app.version,
        "uptime_seconds": uptime,
        "predictor_ready": predictor.using_real_model,
    }

@app.post("/forecast", response_model=ForecastResponse)
async def get_forecast(request: ForecastRequest):
    # FIX 4: Minimum array length enforcement
    if len(request.ohlcv) < 5:
        raise HTTPException(
            status_code=400,
            detail="Insufficient history. Minimum 5 bars required."
        )

    try:
        data = [bar.model_dump() for bar in request.ohlcv]
        df = pd.DataFrame(data)
        df['timestamp'] = pd.to_datetime(df['timestamp'], format='ISO8601')
        df = df.set_index('timestamp')

        # FIX 2: Dynamic timeframe instead of hardcoded '1min'
        last_ts = df.index[-1]
        freq = request.timeframe
        y_timestamp = pd.date_range(
            start=last_ts + pd.Timedelta(minutes=1),
            periods=request.pred_len,
            freq=freq
        )

        pred_df = predictor.predict(
            df,
            x_timestamp=df.index,
            y_timestamp=y_timestamp,
            pred_len=request.pred_len,
            temperature=request.temperature,
            top_p=request.top_p,
            sample_count=request.sample_count
        )

        pred_df.index = pred_df.index.strftime('%Y-%m-%dT%H:%M:%SZ')
        pred_df.index.name = 'timestamp'
        forecasts = pred_df.reset_index().to_dict(orient='records')

        return ForecastResponse(
            symbol=request.symbol,
            forecasts=forecasts,
            message="Forecast generated using true distribution median"
        )
    except HTTPException:
        raise
    except Exception as e:
        import traceback
        traceback.print_exc()
        raise HTTPException(status_code=500, detail=f"Kronos prediction failed: {str(e)}")

if __name__ == "__main__":
    port = int(os.getenv("KRONOS_PORT", "8000"))
    print(f"[KronosService] Starting on port {port}")
    uvicorn.run(app, host="0.0.0.0", port=port)
