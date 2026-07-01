//! Concentration Tools

pub enum ConcentrationTool {
    Calculator,
    Monitor,
}

impl ConcentrationTool {
    pub fn name(&self) -> &'static str {
        match self {
            ConcentrationTool::Calculator => "Calculator",
            ConcentrationTool::Monitor => "Monitor",
        }
    }
}
