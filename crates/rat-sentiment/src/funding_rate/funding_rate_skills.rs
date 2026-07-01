//! FundingRate Skills

pub enum FundingRateSkill {
    Analysis,
    Scoring,
}

impl FundingRateSkill {
    pub fn name(&self) -> &'static str {
        match self {
            FundingRateSkill::Analysis => "Analysis",
            FundingRateSkill::Scoring => "Scoring",
        }
    }
}
