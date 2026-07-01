//! FearGreed Skills

pub enum FearGreedSkill {
    Analysis,
    Scoring,
}

impl FearGreedSkill {
    pub fn name(&self) -> &'static str {
        match self {
            FearGreedSkill::Analysis => "Analysis",
            FearGreedSkill::Scoring => "Scoring",
        }
    }
}
