//! SignalGeneration Rules

pub enum SignalGenerationRule {
    MinQuality(f64),
    MaxLatency(u64),
}

impl SignalGenerationRule {
    pub fn name(&self) -> &'static str {
        match self {
            SignalGenerationRule::MinQuality(_) => "MinQuality",
            SignalGenerationRule::MaxLatency(_) => "MaxLatency",
        }
    }
}
