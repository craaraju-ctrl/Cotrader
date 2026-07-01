//! Swing Rules

pub enum SwingRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl SwingRule {
    pub fn name(&self) -> &'static str {
        match self {
            SwingRule::MinConfluence(_) => "MinConfluence",
            SwingRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
