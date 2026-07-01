//! Donchian Rules

pub enum DonchianRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl DonchianRule {
    pub fn name(&self) -> &'static str {
        match self {
            DonchianRule::OverboughtThreshold(_) => "OverboughtThreshold",
            DonchianRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
