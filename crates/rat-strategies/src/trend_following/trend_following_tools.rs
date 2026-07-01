//! TrendFollowing Tools

pub enum TrendFollowingTool {
    Backtester,
    Optimizer,
}

impl TrendFollowingTool {
    pub fn name(&self) -> &'static str {
        match self {
            TrendFollowingTool::Backtester => "Backtester",
            TrendFollowingTool::Optimizer => "Optimizer",
        }
    }
}
