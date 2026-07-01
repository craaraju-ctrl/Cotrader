//! OptionsSentiment Rules

pub enum OptionsSentimentRule {
    MinConfidence(f64),
    MaxAge(u64),
}

impl OptionsSentimentRule {
    pub fn name(&self) -> &'static str {
        match self {
            OptionsSentimentRule::MinConfidence(_) => "MinConfidence",
            OptionsSentimentRule::MaxAge(_) => "MaxAge",
        }
    }
}
