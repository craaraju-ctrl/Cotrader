//! SocialSentiment Tools

pub enum SocialSentimentTool {
    ApiClient,
    Parser,
}

impl SocialSentimentTool {
    pub fn name(&self) -> &'static str {
        match self {
            SocialSentimentTool::ApiClient => "ApiClient",
            SocialSentimentTool::Parser => "Parser",
        }
    }
}
