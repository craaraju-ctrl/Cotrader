//! TrendFollowing Skills

pub enum TrendFollowingSkill {
    SignalGeneration,
    Backtesting,
}

impl TrendFollowingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            TrendFollowingSkill::SignalGeneration => "SignalGeneration",
            TrendFollowingSkill::Backtesting => "Backtesting",
        }
    }
}
