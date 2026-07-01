//! Scalping Tools

pub enum ScalpingTool {
    Backtester,
    Optimizer,
}

impl ScalpingTool {
    pub fn name(&self) -> &'static str {
        match self {
            ScalpingTool::Backtester => "Backtester",
            ScalpingTool::Optimizer => "Optimizer",
        }
    }
}
