//! Volatile Tools

pub enum VolatileTool {
    Detector,
    Classifier,
}

impl VolatileTool {
    pub fn name(&self) -> &'static str {
        match self {
            VolatileTool::Detector => "Detector",
            VolatileTool::Classifier => "Classifier",
        }
    }
}
