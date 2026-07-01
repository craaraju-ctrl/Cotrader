//! Chart Rules

pub enum ChartRule {
    MinConfidence(f64),
    MinBars(usize),
}

impl ChartRule {
    pub fn name(&self) -> &'static str {
        match self {
            ChartRule::MinConfidence(_) => "MinConfidence",
            ChartRule::MinBars(_) => "MinBars",
        }
    }
}
