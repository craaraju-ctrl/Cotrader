//! FearGreed Rules

pub enum FearGreedRule {
    MinConfidence(f64),
    MaxAge(u64),
}

impl FearGreedRule {
    pub fn name(&self) -> &'static str {
        match self {
            FearGreedRule::MinConfidence(_) => "MinConfidence",
            FearGreedRule::MaxAge(_) => "MaxAge",
        }
    }
}
