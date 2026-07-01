//! Candlestick Tools

pub enum CandlestickTool {
    Detector,
    Validator,
}

impl CandlestickTool {
    pub fn name(&self) -> &'static str {
        match self {
            CandlestickTool::Detector => "Detector",
            CandlestickTool::Validator => "Validator",
        }
    }
}
