//! OptionsSentiment Skills

pub enum OptionsSentimentSkill {
    Analysis,
    Scoring,
}

impl OptionsSentimentSkill {
    pub fn name(&self) -> &'static str {
        match self {
            OptionsSentimentSkill::Analysis => "Analysis",
            OptionsSentimentSkill::Scoring => "Scoring",
        }
    }
}
