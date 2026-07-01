//! Grid Rules

pub enum GridRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl GridRule {
    pub fn name(&self) -> &'static str {
        match self {
            GridRule::MinConfluence(_) => "MinConfluence",
            GridRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
