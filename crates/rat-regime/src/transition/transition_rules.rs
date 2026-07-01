//! Transition Rules

pub enum TransitionRule {
    MinConfidence(f64),
    MaxDuration(u64),
}

impl TransitionRule {
    pub fn name(&self) -> &'static str {
        match self {
            TransitionRule::MinConfidence(_) => "MinConfidence",
            TransitionRule::MaxDuration(_) => "MaxDuration",
        }
    }
}
