//! SocialSentiment Tools

pub enum SocialSentimentTool {
    Analyzer,
    Scorer,
}

impl SocialSentimentTool {
    pub fn name(&self) -> &'static str {
        match self {
            SocialSentimentTool::Analyzer => "Analyzer",
            SocialSentimentTool::Scorer => "Scorer",
        }
    }
}
