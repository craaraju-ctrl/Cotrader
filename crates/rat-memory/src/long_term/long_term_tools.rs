//! LongTerm Tools

pub enum LongTermTool {
    Storage,
    Index,
}

impl LongTermTool {
    pub fn name(&self) -> &'static str {
        match self {
            LongTermTool::Storage => "Storage",
            LongTermTool::Index => "Index",
        }
    }
}
