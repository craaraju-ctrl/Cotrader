//! Adx Rules

pub enum AdxRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl AdxRule {
    pub fn name(&self) -> &'static str {
        match self {
            AdxRule::OverboughtThreshold(_) => "OverboughtThreshold",
            AdxRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
