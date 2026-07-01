//! FundingRate Tools

pub enum FundingRateTool {
    Analyzer,
    Scorer,
}

impl FundingRateTool {
    pub fn name(&self) -> &'static str {
        match self {
            FundingRateTool::Analyzer => "Analyzer",
            FundingRateTool::Scorer => "Scorer",
        }
    }
}
