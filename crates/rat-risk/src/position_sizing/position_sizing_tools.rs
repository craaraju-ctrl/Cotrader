//! PositionSizing Tools

pub enum PositionSizingTool {
    Calculator,
    Monitor,
}

impl PositionSizingTool {
    pub fn name(&self) -> &'static str {
        match self {
            PositionSizingTool::Calculator => "Calculator",
            PositionSizingTool::Monitor => "Monitor",
        }
    }
}
