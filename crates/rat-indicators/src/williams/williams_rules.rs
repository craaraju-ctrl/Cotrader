//! Williams Rules

pub enum WilliamsRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl WilliamsRule {
    pub fn name(&self) -> &'static str {
        match self {
            WilliamsRule::OverboughtThreshold(_) => "OverboughtThreshold",
            WilliamsRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
