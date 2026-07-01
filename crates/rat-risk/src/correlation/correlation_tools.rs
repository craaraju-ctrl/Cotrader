//! Correlation Tools

pub enum CorrelationTool {
    Calculator,
    Monitor,
}

impl CorrelationTool {
    pub fn name(&self) -> &'static str {
        match self {
            CorrelationTool::Calculator => "Calculator",
            CorrelationTool::Monitor => "Monitor",
        }
    }
}
