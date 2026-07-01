//! Adx Tools

pub enum AdxTool {
    DataFetcher,
    Calculator,
}

impl AdxTool {
    pub fn name(&self) -> &'static str {
        match self {
            AdxTool::DataFetcher => "DataFetcher",
            AdxTool::Calculator => "Calculator",
        }
    }
}
