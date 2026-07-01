//! FearGreed Tools

pub enum FearGreedTool {
    Analyzer,
    Scorer,
}

impl FearGreedTool {
    pub fn name(&self) -> &'static str {
        match self {
            FearGreedTool::Analyzer => "Analyzer",
            FearGreedTool::Scorer => "Scorer",
        }
    }
}
