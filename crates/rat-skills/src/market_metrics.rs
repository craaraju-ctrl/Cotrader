//! Market Metrics — 26 indicators (RSI, MACD, ATR, Bollinger, etc.)

pub struct MarketMetricsMeter;

impl MarketMetricsMeter {
    pub fn name() -> &'static str { "MarketMetricsMeter" }
    pub fn indicator_count() -> usize { 26 }
}
