//! Pivot Rules

pub enum PivotRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl PivotRule {
    pub fn name(&self) -> &'static str {
        match self {
            PivotRule::OverboughtThreshold(_) => "OverboughtThreshold",
            PivotRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
