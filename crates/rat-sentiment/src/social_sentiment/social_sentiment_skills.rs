//! SocialSentiment Skills

pub enum SocialSentimentSkill {
    Analysis,
    Scoring,
}

impl SocialSentimentSkill {
    pub fn name(&self) -> &'static str {
        match self {
            SocialSentimentSkill::Analysis => "Analysis",
            SocialSentimentSkill::Scoring => "Scoring",
        }
    }
}
