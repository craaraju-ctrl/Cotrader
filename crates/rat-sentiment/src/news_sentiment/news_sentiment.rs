//! NewsSentiment

pub struct NewsSentimentAnalyzer;

impl NewsSentimentAnalyzer {
    pub fn name() -> &'static str { "NewsSentimentAnalyzer" }
    pub fn analyze(&self) -> f64 { 0.0 }
}
