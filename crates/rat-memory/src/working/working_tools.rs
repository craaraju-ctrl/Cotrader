//! Working Tools

pub enum WorkingTool {
    Storage,
    Index,
}

impl WorkingTool {
    pub fn name(&self) -> &'static str {
        match self {
            WorkingTool::Storage => "Storage",
            WorkingTool::Index => "Index",
        }
    }
}
