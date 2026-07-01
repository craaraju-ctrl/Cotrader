//! Ranging Tools

pub enum RangingTool {
    Detector,
    Classifier,
}

impl RangingTool {
    pub fn name(&self) -> &'static str {
        match self {
            RangingTool::Detector => "Detector",
            RangingTool::Classifier => "Classifier",
        }
    }
}
