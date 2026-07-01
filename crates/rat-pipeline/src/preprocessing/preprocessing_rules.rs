//! Preprocessing Rules

pub enum PreprocessingRule {
    MinQuality(f64),
    MaxLatency(u64),
}

impl PreprocessingRule {
    pub fn name(&self) -> &'static str {
        match self {
            PreprocessingRule::MinQuality(_) => "MinQuality",
            PreprocessingRule::MaxLatency(_) => "MaxLatency",
        }
    }
}
