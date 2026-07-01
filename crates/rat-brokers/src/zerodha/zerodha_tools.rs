//! Zerodha Tools

pub enum ZerodhaTool {
    RestClient,
    WebSocketClient,
}

impl ZerodhaTool {
    pub fn name(&self) -> &'static str {
        match self {
            ZerodhaTool::RestClient => "RestClient",
            ZerodhaTool::WebSocketClient => "WebSocketClient",
        }
    }
}
