//! Harmonic Tools

pub enum HarmonicTool {
    Detector,
    Validator,
}

impl HarmonicTool {
    pub fn name(&self) -> &'static str {
        match self {
            HarmonicTool::Detector => "Detector",
            HarmonicTool::Validator => "Validator",
        }
    }
}
