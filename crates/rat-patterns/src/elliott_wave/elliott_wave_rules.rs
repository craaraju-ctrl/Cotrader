//! ElliottWave Rules

pub enum ElliottWaveRule {
    MinConfidence(f64),
    MinBars(usize),
}

impl ElliottWaveRule {
    pub fn name(&self) -> &'static str {
        match self {
            ElliottWaveRule::MinConfidence(_) => "MinConfidence",
            ElliottWaveRule::MinBars(_) => "MinBars",
        }
    }
}
