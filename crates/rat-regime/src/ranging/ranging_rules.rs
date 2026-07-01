//! Ranging Rules

pub enum RangingRule {
    MinConfidence(f64),
    MaxDuration(u64),
}

impl RangingRule {
    pub fn name(&self) -> &'static str {
        match self {
            RangingRule::MinConfidence(_) => "MinConfidence",
            RangingRule::MaxDuration(_) => "MaxDuration",
        }
    }
}
