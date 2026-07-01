//! OnChain Rules

pub enum OnChainRule {
    MaxAge(u64),
    MinRelevance(f64),
}

impl OnChainRule {
    pub fn name(&self) -> &'static str {
        match self {
            OnChainRule::MaxAge(_) => "MaxAge",
            OnChainRule::MinRelevance(_) => "MinRelevance",
        }
    }
}
