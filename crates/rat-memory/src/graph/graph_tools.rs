//! Graph Tools

pub enum GraphTool {
    Storage,
    Index,
}

impl GraphTool {
    pub fn name(&self) -> &'static str {
        match self {
            GraphTool::Storage => "Storage",
            GraphTool::Index => "Index",
        }
    }
}
