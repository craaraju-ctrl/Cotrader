//! Wyckoff Rules

pub enum WyckoffRule {
    MinConfidence(f64),
    MinBars(usize),
}

impl WyckoffRule {
    pub fn name(&self) -> &'static str {
        match self {
            WyckoffRule::MinConfidence(_) => "MinConfidence",
            WyckoffRule::MinBars(_) => "MinBars",
        }
    }
}
