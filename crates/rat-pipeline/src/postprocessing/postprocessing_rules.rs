//! Postprocessing Rules

pub enum PostprocessingRule {
    MinQuality(f64),
    MaxLatency(u64),
}

impl PostprocessingRule {
    pub fn name(&self) -> &'static str {
        match self {
            PostprocessingRule::MinQuality(_) => "MinQuality",
            PostprocessingRule::MaxLatency(_) => "MaxLatency",
        }
    }
}
