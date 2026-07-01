//! MarketMaking Tools

pub enum MarketMakingTool {
    Backtester,
    Optimizer,
}

impl MarketMakingTool {
    pub fn name(&self) -> &'static str {
        match self {
            MarketMakingTool::Backtester => "Backtester",
            MarketMakingTool::Optimizer => "Optimizer",
        }
    }
}
