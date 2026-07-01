//! Execution Rules

pub enum ExecutionRule {
    MinQuality(f64),
    MaxLatency(u64),
}

impl ExecutionRule {
    pub fn name(&self) -> &'static str {
        match self {
            ExecutionRule::MinQuality(_) => "MinQuality",
            ExecutionRule::MaxLatency(_) => "MaxLatency",
        }
    }
}
