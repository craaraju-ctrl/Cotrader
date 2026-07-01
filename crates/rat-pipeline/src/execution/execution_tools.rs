//! Execution Tools

pub enum ExecutionTool {
    Processor,
    Validator,
}

impl ExecutionTool {
    pub fn name(&self) -> &'static str {
        match self {
            ExecutionTool::Processor => "Processor",
            ExecutionTool::Validator => "Validator",
        }
    }
}
