//! Trending Rules

pub enum TrendingRule {
    MinConfidence(f64),
    MaxDuration(u64),
}

impl TrendingRule {
    pub fn name(&self) -> &'static str {
        match self {
            TrendingRule::MinConfidence(_) => "MinConfidence",
            TrendingRule::MaxDuration(_) => "MaxDuration",
        }
    }
}
