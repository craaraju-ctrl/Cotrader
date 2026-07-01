//! Preprocessing Tools

pub enum PreprocessingTool {
    Processor,
    Validator,
}

impl PreprocessingTool {
    pub fn name(&self) -> &'static str {
        match self {
            PreprocessingTool::Processor => "Processor",
            PreprocessingTool::Validator => "Validator",
        }
    }
}
