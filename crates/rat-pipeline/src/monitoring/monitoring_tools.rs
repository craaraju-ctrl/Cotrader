//! Monitoring Tools

pub enum MonitoringTool {
    Processor,
    Validator,
}

impl MonitoringTool {
    pub fn name(&self) -> &'static str {
        match self {
            MonitoringTool::Processor => "Processor",
            MonitoringTool::Validator => "Validator",
        }
    }
}
