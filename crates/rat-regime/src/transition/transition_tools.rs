//! Transition Tools

pub enum TransitionTool {
    Detector,
    Classifier,
}

impl TransitionTool {
    pub fn name(&self) -> &'static str {
        match self {
            TransitionTool::Detector => "Detector",
            TransitionTool::Classifier => "Classifier",
        }
    }
}
