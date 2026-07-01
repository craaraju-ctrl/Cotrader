//! Binance Tools

pub enum BinanceTool {
    RestClient,
    WebSocketClient,
}

impl BinanceTool {
    pub fn name(&self) -> &'static str {
        match self {
            BinanceTool::RestClient => "RestClient",
            BinanceTool::WebSocketClient => "WebSocketClient",
        }
    }
}
