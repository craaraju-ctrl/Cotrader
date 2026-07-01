//! Rsi Rules

pub enum RsiRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl RsiRule {
    pub fn name(&self) -> &'static str {
        match self {
            RsiRule::OverboughtThreshold(_) => "OverboughtThreshold",
            RsiRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
