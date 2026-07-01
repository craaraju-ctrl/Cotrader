//! Grid Tools

pub enum GridTool {
    Backtester,
    Optimizer,
}

impl GridTool {
    pub fn name(&self) -> &'static str {
        match self {
            GridTool::Backtester => "Backtester",
            GridTool::Optimizer => "Optimizer",
        }
    }
}
