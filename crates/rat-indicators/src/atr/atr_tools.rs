//! Atr Tools

pub enum AtrTool {
    DataFetcher,
    Calculator,
}

impl AtrTool {
    pub fn name(&self) -> &'static str {
        match self {
            AtrTool::DataFetcher => "DataFetcher",
            AtrTool::Calculator => "Calculator",
        }
    }
}
