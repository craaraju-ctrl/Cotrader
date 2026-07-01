//! Pairs Tools

pub enum PairsTool {
    Backtester,
    Optimizer,
}

impl PairsTool {
    pub fn name(&self) -> &'static str {
        match self {
            PairsTool::Backtester => "Backtester",
            PairsTool::Optimizer => "Optimizer",
        }
    }
}
