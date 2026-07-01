//! Pairs Rules

pub enum PairsRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl PairsRule {
    pub fn name(&self) -> &'static str {
        match self {
            PairsRule::MinConfluence(_) => "MinConfluence",
            PairsRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
