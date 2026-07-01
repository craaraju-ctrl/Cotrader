//! Cci Tools

pub enum CciTool {
    DataFetcher,
    Calculator,
}

impl CciTool {
    pub fn name(&self) -> &'static str {
        match self {
            CciTool::DataFetcher => "DataFetcher",
            CciTool::Calculator => "Calculator",
        }
    }
}
