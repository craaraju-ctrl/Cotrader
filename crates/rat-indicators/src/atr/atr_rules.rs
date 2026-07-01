//! Atr Rules

pub enum AtrRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl AtrRule {
    pub fn name(&self) -> &'static str {
        match self {
            AtrRule::OverboughtThreshold(_) => "OverboughtThreshold",
            AtrRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
