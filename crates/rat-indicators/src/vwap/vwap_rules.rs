//! Vwap Rules

pub enum VwapRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl VwapRule {
    pub fn name(&self) -> &'static str {
        match self {
            VwapRule::OverboughtThreshold(_) => "OverboughtThreshold",
            VwapRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
