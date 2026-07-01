//! Donchian Tools

pub enum DonchianTool {
    DataFetcher,
    Calculator,
}

impl DonchianTool {
    pub fn name(&self) -> &'static str {
        match self {
            DonchianTool::DataFetcher => "DataFetcher",
            DonchianTool::Calculator => "Calculator",
        }
    }
}
