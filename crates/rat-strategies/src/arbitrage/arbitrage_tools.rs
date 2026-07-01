//! Arbitrage Tools

pub enum ArbitrageTool {
    Backtester,
    Optimizer,
}

impl ArbitrageTool {
    pub fn name(&self) -> &'static str {
        match self {
            ArbitrageTool::Backtester => "Backtester",
            ArbitrageTool::Optimizer => "Optimizer",
        }
    }
}
