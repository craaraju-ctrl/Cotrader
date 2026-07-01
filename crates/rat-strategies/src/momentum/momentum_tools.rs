//! Momentum Tools

pub enum MomentumTool {
    Backtester,
    Optimizer,
}

impl MomentumTool {
    pub fn name(&self) -> &'static str {
        match self {
            MomentumTool::Backtester => "Backtester",
            MomentumTool::Optimizer => "Optimizer",
        }
    }
}
