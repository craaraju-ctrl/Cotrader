//! Trending Tools

pub enum TrendingTool {
    Detector,
    Classifier,
}

impl TrendingTool {
    pub fn name(&self) -> &'static str {
        match self {
            TrendingTool::Detector => "Detector",
            TrendingTool::Classifier => "Classifier",
        }
    }
}
