//! NewsSentiment Rules

pub enum NewsSentimentRule {
    MinConfidence(f64),
    MaxAge(u64),
}

impl NewsSentimentRule {
    pub fn name(&self) -> &'static str {
        match self {
            NewsSentimentRule::MinConfidence(_) => "MinConfidence",
            NewsSentimentRule::MaxAge(_) => "MaxAge",
        }
    }
}
