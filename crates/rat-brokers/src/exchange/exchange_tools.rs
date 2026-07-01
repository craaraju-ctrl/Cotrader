//! Exchange Tools

pub enum ExchangeTool {
    RestClient,
    WebSocketClient,
}

impl ExchangeTool {
    pub fn name(&self) -> &'static str {
        match self {
            ExchangeTool::RestClient => "RestClient",
            ExchangeTool::WebSocketClient => "WebSocketClient",
        }
    }
}
