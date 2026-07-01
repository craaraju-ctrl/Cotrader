//! Candlestick Skills

pub enum CandlestickSkill {
    Detection,
    Confirmation,
}

impl CandlestickSkill {
    pub fn name(&self) -> &'static str {
        match self {
            CandlestickSkill::Detection => "Detection",
            CandlestickSkill::Confirmation => "Confirmation",
        }
    }
}
