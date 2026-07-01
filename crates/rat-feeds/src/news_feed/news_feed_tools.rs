//! NewsFeed Tools

pub enum NewsFeedTool {
    ApiClient,
    Parser,
}

impl NewsFeedTool {
    pub fn name(&self) -> &'static str {
        match self {
            NewsFeedTool::ApiClient => "ApiClient",
            NewsFeedTool::Parser => "Parser",
        }
    }
}
