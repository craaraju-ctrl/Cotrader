//! NewsSentiment Skills

pub enum NewsSentimentSkill {
    Analysis,
    Scoring,
}

impl NewsSentimentSkill {
    pub fn name(&self) -> &'static str {
        match self {
            NewsSentimentSkill::Analysis => "Analysis",
            NewsSentimentSkill::Scoring => "Scoring",
        }
    }
}
