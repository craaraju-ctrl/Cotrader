//! TrendFollowing Rules

pub enum TrendFollowingRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl TrendFollowingRule {
    pub fn name(&self) -> &'static str {
        match self {
            TrendFollowingRule::MinConfluence(_) => "MinConfluence",
            TrendFollowingRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
