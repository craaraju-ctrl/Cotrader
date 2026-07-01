//! Chart Tools

pub enum ChartTool {
    Detector,
    Validator,
}

impl ChartTool {
    pub fn name(&self) -> &'static str {
        match self {
            ChartTool::Detector => "Detector",
            ChartTool::Validator => "Validator",
        }
    }
}
