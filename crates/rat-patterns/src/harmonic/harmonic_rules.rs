//! Harmonic Rules

pub enum HarmonicRule {
    MinConfidence(f64),
    MinBars(usize),
}

impl HarmonicRule {
    pub fn name(&self) -> &'static str {
        match self {
            HarmonicRule::MinConfidence(_) => "MinConfidence",
            HarmonicRule::MinBars(_) => "MinBars",
        }
    }
}
