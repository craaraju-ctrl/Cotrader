//! Bollinger Tools

pub enum BollingerTool {
    DataFetcher,
    Calculator,
}

impl BollingerTool {
    pub fn name(&self) -> &'static str {
        match self {
            BollingerTool::DataFetcher => "DataFetcher",
            BollingerTool::Calculator => "Calculator",
        }
    }
}
