//! Ichimoku Tools

pub enum IchimokuTool {
    DataFetcher,
    Calculator,
}

impl IchimokuTool {
    pub fn name(&self) -> &'static str {
        match self {
            IchimokuTool::DataFetcher => "DataFetcher",
            IchimokuTool::Calculator => "Calculator",
        }
    }
}
