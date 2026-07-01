//! Swing Tools

pub enum SwingTool {
    Backtester,
    Optimizer,
}

impl SwingTool {
    pub fn name(&self) -> &'static str {
        match self {
            SwingTool::Backtester => "Backtester",
            SwingTool::Optimizer => "Optimizer",
        }
    }
}
