//! OptionsFlow Tools

pub enum OptionsFlowTool {
    ApiClient,
    Parser,
}

impl OptionsFlowTool {
    pub fn name(&self) -> &'static str {
        match self {
            OptionsFlowTool::ApiClient => "ApiClient",
            OptionsFlowTool::Parser => "Parser",
        }
    }
}
