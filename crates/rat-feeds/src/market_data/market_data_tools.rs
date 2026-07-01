//! MarketData Tools

pub enum MarketDataTool {
    ApiClient,
    Parser,
}

impl MarketDataTool {
    pub fn name(&self) -> &'static str {
        match self {
            MarketDataTool::ApiClient => "ApiClient",
            MarketDataTool::Parser => "Parser",
        }
    }
}
