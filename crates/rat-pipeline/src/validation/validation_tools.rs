//! Validation Tools

pub enum ValidationTool {
    Processor,
    Validator,
}

impl ValidationTool {
    pub fn name(&self) -> &'static str {
        match self {
            ValidationTool::Processor => "Processor",
            ValidationTool::Validator => "Validator",
        }
    }
}
