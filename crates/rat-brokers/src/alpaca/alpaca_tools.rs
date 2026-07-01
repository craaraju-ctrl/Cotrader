//! Alpaca Tools

pub enum AlpacaTool {
    RestClient,
    WebSocketClient,
}

impl AlpacaTool {
    pub fn name(&self) -> &'static str {
        match self {
            AlpacaTool::RestClient => "RestClient",
            AlpacaTool::WebSocketClient => "WebSocketClient",
        }
    }
}
