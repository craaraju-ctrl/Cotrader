//! SocialSentiment Rules

pub enum SocialSentimentRule {
    MinConfidence(f64),
    MaxAge(u64),
}

impl SocialSentimentRule {
    pub fn name(&self) -> &'static str {
        match self {
            SocialSentimentRule::MinConfidence(_) => "MinConfidence",
            SocialSentimentRule::MaxAge(_) => "MaxAge",
        }
    }
}
