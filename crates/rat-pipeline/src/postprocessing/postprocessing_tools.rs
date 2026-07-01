//! Postprocessing Tools

pub enum PostprocessingTool {
    Processor,
    Validator,
}

impl PostprocessingTool {
    pub fn name(&self) -> &'static str {
        match self {
            PostprocessingTool::Processor => "Processor",
            PostprocessingTool::Validator => "Validator",
        }
    }
}
