//! MarketMaking Rules

pub enum MarketMakingRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl MarketMakingRule {
    pub fn name(&self) -> &'static str {
        match self {
            MarketMakingRule::MinConfluence(_) => "MinConfluence",
            MarketMakingRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
