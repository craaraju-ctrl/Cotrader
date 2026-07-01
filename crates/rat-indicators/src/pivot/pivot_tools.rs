//! Pivot Tools

pub enum PivotTool {
    DataFetcher,
    Calculator,
}

impl PivotTool {
    pub fn name(&self) -> &'static str {
        match self {
            PivotTool::DataFetcher => "DataFetcher",
            PivotTool::Calculator => "Calculator",
        }
    }
}
