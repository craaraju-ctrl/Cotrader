//! MeanReversion Rules

pub enum MeanReversionRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl MeanReversionRule {
    pub fn name(&self) -> &'static str {
        match self {
            MeanReversionRule::MinConfluence(_) => "MinConfluence",
            MeanReversionRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
