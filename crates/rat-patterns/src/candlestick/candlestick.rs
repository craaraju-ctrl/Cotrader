//! Candlestick Patterns

pub struct CandlestickDetector;

impl CandlestickDetector {
    pub fn name() -> &'static str { "CandlestickDetector" }
    pub fn detect(&self) -> Vec<String> { vec![] }
}
