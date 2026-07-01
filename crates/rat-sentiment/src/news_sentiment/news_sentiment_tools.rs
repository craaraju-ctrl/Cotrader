//! NewsSentiment Tools

pub enum NewsSentimentTool {
    Analyzer,
    Scorer,
}

impl NewsSentimentTool {
    pub fn name(&self) -> &'static str {
        match self {
            NewsSentimentTool::Analyzer => "Analyzer",
            NewsSentimentTool::Scorer => "Scorer",
        }
    }
}
