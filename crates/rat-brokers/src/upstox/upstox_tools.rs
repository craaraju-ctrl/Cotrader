//! Upstox Tools

pub enum UpstoxTool {
    RestClient,
    WebSocketClient,
}

impl UpstoxTool {
    pub fn name(&self) -> &'static str {
        match self {
            UpstoxTool::RestClient => "RestClient",
            UpstoxTool::WebSocketClient => "WebSocketClient",
        }
    }
}
