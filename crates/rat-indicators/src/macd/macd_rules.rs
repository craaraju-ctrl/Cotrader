//! Macd Rules

pub enum MacdRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl MacdRule {
    pub fn name(&self) -> &'static str {
        match self {
            MacdRule::OverboughtThreshold(_) => "OverboughtThreshold",
            MacdRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
