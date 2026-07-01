//! SocialSentiment Feed

pub struct SocialSentimentFeed;

impl SocialSentimentFeed {
    pub fn name() -> &'static str { "SocialSentimentFeed" }
    pub fn fetch(&self) -> Vec<String> { vec![] }
}
