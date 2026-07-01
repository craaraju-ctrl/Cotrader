//! Breakout Rules

pub enum BreakoutRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl BreakoutRule {
    pub fn name(&self) -> &'static str {
        match self {
            BreakoutRule::MinConfluence(_) => "MinConfluence",
            BreakoutRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
