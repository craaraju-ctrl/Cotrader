//! Cci Rules

pub enum CciRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl CciRule {
    pub fn name(&self) -> &'static str {
        match self {
            CciRule::OverboughtThreshold(_) => "OverboughtThreshold",
            CciRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
