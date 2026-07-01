//! Episodic Tools

pub enum EpisodicTool {
    Storage,
    Index,
}

impl EpisodicTool {
    pub fn name(&self) -> &'static str {
        match self {
            EpisodicTool::Storage => "Storage",
            EpisodicTool::Index => "Index",
        }
    }
}
