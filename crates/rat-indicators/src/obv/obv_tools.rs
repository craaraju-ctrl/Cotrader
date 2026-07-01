//! Obv Tools

pub enum ObvTool {
    DataFetcher,
    Calculator,
}

impl ObvTool {
    pub fn name(&self) -> &'static str {
        match self {
            ObvTool::DataFetcher => "DataFetcher",
            ObvTool::Calculator => "Calculator",
        }
    }
}
