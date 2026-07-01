//! Fivepaisa Tools

pub enum FivepaisaTool {
    RestClient,
    WebSocketClient,
}

impl FivepaisaTool {
    pub fn name(&self) -> &'static str {
        match self {
            FivepaisaTool::RestClient => "RestClient",
            FivepaisaTool::WebSocketClient => "WebSocketClient",
        }
    }
}
