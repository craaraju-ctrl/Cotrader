//! Scalping Rules

pub enum ScalpingRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl ScalpingRule {
    pub fn name(&self) -> &'static str {
        match self {
            ScalpingRule::MinConfluence(_) => "MinConfluence",
            ScalpingRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
