//! Validation Rules

pub enum ValidationRule {
    MinQuality(f64),
    MaxLatency(u64),
}

impl ValidationRule {
    pub fn name(&self) -> &'static str {
        match self {
            ValidationRule::MinQuality(_) => "MinQuality",
            ValidationRule::MaxLatency(_) => "MaxLatency",
        }
    }
}
