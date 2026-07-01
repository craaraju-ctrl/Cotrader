//! Breakout Tools

pub enum BreakoutTool {
    Backtester,
    Optimizer,
}

impl BreakoutTool {
    pub fn name(&self) -> &'static str {
        match self {
            BreakoutTool::Backtester => "Backtester",
            BreakoutTool::Optimizer => "Optimizer",
        }
    }
}
