//! FundingRate Rules

pub enum FundingRateRule {
    MinConfidence(f64),
    MaxAge(u64),
}

impl FundingRateRule {
    pub fn name(&self) -> &'static str {
        match self {
            FundingRateRule::MinConfidence(_) => "MinConfidence",
            FundingRateRule::MaxAge(_) => "MaxAge",
        }
    }
}
