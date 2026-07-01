//! MeanReversion Tools

pub enum MeanReversionTool {
    Backtester,
    Optimizer,
}

impl MeanReversionTool {
    pub fn name(&self) -> &'static str {
        match self {
            MeanReversionTool::Backtester => "Backtester",
            MeanReversionTool::Optimizer => "Optimizer",
        }
    }
}
