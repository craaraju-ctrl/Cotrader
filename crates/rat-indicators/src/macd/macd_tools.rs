//! Macd Tools

pub enum MacdTool {
    DataFetcher,
    Calculator,
}

impl MacdTool {
    pub fn name(&self) -> &'static str {
        match self {
            MacdTool::DataFetcher => "DataFetcher",
            MacdTool::Calculator => "Calculator",
        }
    }
}
