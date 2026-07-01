//! Volatile Rules

pub enum VolatileRule {
    MinConfidence(f64),
    MaxDuration(u64),
}

impl VolatileRule {
    pub fn name(&self) -> &'static str {
        match self {
            VolatileRule::MinConfidence(_) => "MinConfidence",
            VolatileRule::MaxDuration(_) => "MaxDuration",
        }
    }
}
