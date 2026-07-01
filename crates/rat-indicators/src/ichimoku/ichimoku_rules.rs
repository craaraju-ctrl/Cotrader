//! Ichimoku Rules

pub enum IchimokuRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl IchimokuRule {
    pub fn name(&self) -> &'static str {
        match self {
            IchimokuRule::OverboughtThreshold(_) => "OverboughtThreshold",
            IchimokuRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
