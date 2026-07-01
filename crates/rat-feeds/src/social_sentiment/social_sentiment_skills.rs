//! SocialSentiment Skills

pub enum SocialSentimentSkill {
    Fetch,
    Parse,
}

impl SocialSentimentSkill {
    pub fn name(&self) -> &'static str {
        match self {
            SocialSentimentSkill::Fetch => "Fetch",
            SocialSentimentSkill::Parse => "Parse",
        }
    }
}
