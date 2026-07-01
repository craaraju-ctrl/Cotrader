//! Semantic Tools

pub enum SemanticTool {
    Storage,
    Index,
}

impl SemanticTool {
    pub fn name(&self) -> &'static str {
        match self {
            SemanticTool::Storage => "Storage",
            SemanticTool::Index => "Index",
        }
    }
}
