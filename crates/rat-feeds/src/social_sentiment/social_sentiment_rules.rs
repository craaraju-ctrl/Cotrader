//! SocialSentiment Rules

pub enum SocialSentimentRule {
    MaxAge(u64),
    MinRelevance(f64),
}

impl SocialSentimentRule {
    pub fn name(&self) -> &'static str {
        match self {
            SocialSentimentRule::MaxAge(_) => "MaxAge",
            SocialSentimentRule::MinRelevance(_) => "MinRelevance",
        }
    }
}
