//! Correlation Rules

pub enum CorrelationRule {
    MaxThreshold(f64),
    WarningLevel(f64),
}

impl CorrelationRule {
    pub fn name(&self) -> &'static str {
        match self {
            CorrelationRule::MaxThreshold(_) => "MaxThreshold",
            CorrelationRule::WarningLevel(_) => "WarningLevel",
        }
    }
}
