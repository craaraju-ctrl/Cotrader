//! Keltner Tools

pub enum KeltnerTool {
    DataFetcher,
    Calculator,
}

impl KeltnerTool {
    pub fn name(&self) -> &'static str {
        match self {
            KeltnerTool::DataFetcher => "DataFetcher",
            KeltnerTool::Calculator => "Calculator",
        }
    }
}
