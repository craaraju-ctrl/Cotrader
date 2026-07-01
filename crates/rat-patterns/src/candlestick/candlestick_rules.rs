//! Candlestick Rules

pub enum CandlestickRule {
    MinConfidence(f64),
    MinBars(usize),
}

impl CandlestickRule {
    pub fn name(&self) -> &'static str {
        match self {
            CandlestickRule::MinConfidence(_) => "MinConfidence",
            CandlestickRule::MinBars(_) => "MinBars",
        }
    }
}
