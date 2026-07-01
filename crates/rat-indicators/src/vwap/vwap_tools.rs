//! Vwap Tools

pub enum VwapTool {
    DataFetcher,
    Calculator,
}

impl VwapTool {
    pub fn name(&self) -> &'static str {
        match self {
            VwapTool::DataFetcher => "DataFetcher",
            VwapTool::Calculator => "Calculator",
        }
    }
}
