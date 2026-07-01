//! Rsi Tools

pub enum RsiTool {
    DataFetcher,
    Calculator,
}

impl RsiTool {
    pub fn name(&self) -> &'static str {
        match self {
            RsiTool::DataFetcher => "DataFetcher",
            RsiTool::Calculator => "Calculator",
        }
    }
}
