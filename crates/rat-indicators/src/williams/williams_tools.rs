//! Williams Tools

pub enum WilliamsTool {
    DataFetcher,
    Calculator,
}

impl WilliamsTool {
    pub fn name(&self) -> &'static str {
        match self {
            WilliamsTool::DataFetcher => "DataFetcher",
            WilliamsTool::Calculator => "Calculator",
        }
    }
}
