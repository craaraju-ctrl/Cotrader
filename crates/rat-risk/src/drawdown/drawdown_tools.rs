//! Drawdown Tools

pub enum DrawdownTool {
    Calculator,
    Monitor,
}

impl DrawdownTool {
    pub fn name(&self) -> &'static str {
        match self {
            DrawdownTool::Calculator => "Calculator",
            DrawdownTool::Monitor => "Monitor",
        }
    }
}
