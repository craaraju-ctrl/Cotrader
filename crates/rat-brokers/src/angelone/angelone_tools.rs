//! Angelone Tools

pub enum AngeloneTool {
    RestClient,
    WebSocketClient,
}

impl AngeloneTool {
    pub fn name(&self) -> &'static str {
        match self {
            AngeloneTool::RestClient => "RestClient",
            AngeloneTool::WebSocketClient => "WebSocketClient",
        }
    }
}
