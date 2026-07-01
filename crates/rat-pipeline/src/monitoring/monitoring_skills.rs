//! Monitoring Skills

pub enum MonitoringSkill {
    Processing,
    Filtering,
}

impl MonitoringSkill {
    pub fn name(&self) -> &'static str {
        match self {
            MonitoringSkill::Processing => "Processing",
            MonitoringSkill::Filtering => "Filtering",
        }
    }
}
