//! OnChain Tools

pub enum OnChainTool {
    ApiClient,
    Parser,
}

impl OnChainTool {
    pub fn name(&self) -> &'static str {
        match self {
            OnChainTool::ApiClient => "ApiClient",
            OnChainTool::Parser => "Parser",
        }
    }
}
