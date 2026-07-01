//! Wyckoff Tools

pub enum WyckoffTool {
    Detector,
    Validator,
}

impl WyckoffTool {
    pub fn name(&self) -> &'static str {
        match self {
            WyckoffTool::Detector => "Detector",
            WyckoffTool::Validator => "Validator",
        }
    }
}
