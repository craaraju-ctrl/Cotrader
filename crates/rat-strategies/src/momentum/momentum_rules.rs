//! Momentum Rules

pub enum MomentumRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl MomentumRule {
    pub fn name(&self) -> &'static str {
        match self {
            MomentumRule::MinConfluence(_) => "MinConfluence",
            MomentumRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
