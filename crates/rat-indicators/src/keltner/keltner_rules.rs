//! Keltner Rules

pub enum KeltnerRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl KeltnerRule {
    pub fn name(&self) -> &'static str {
        match self {
            KeltnerRule::OverboughtThreshold(_) => "OverboughtThreshold",
            KeltnerRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
