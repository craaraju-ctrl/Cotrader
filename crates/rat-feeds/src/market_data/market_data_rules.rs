//! MarketData Rules

pub enum MarketDataRule {
    MaxAge(u64),
    MinRelevance(f64),
}

impl MarketDataRule {
    pub fn name(&self) -> &'static str {
        match self {
            MarketDataRule::MaxAge(_) => "MaxAge",
            MarketDataRule::MinRelevance(_) => "MinRelevance",
        }
    }
}
