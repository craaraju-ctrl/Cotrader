//! Obv Rules

pub enum ObvRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl ObvRule {
    pub fn name(&self) -> &'static str {
        match self {
            ObvRule::OverboughtThreshold(_) => "OverboughtThreshold",
            ObvRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
