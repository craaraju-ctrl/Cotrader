//! SignalGeneration Tools

pub enum SignalGenerationTool {
    Processor,
    Validator,
}

impl SignalGenerationTool {
    pub fn name(&self) -> &'static str {
        match self {
            SignalGenerationTool::Processor => "Processor",
            SignalGenerationTool::Validator => "Validator",
        }
    }
}
