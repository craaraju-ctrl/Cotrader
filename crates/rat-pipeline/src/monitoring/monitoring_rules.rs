//! Monitoring Rules

pub enum MonitoringRule {
    MinQuality(f64),
    MaxLatency(u64),
}

impl MonitoringRule {
    pub fn name(&self) -> &'static str {
        match self {
            MonitoringRule::MinQuality(_) => "MinQuality",
            MonitoringRule::MaxLatency(_) => "MaxLatency",
        }
    }
}
