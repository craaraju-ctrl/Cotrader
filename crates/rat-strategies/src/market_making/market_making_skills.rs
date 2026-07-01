//! MarketMaking Skills

pub enum MarketMakingSkill {
    SignalGeneration,
    Backtesting,
}

impl MarketMakingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            MarketMakingSkill::SignalGeneration => "SignalGeneration",
            MarketMakingSkill::Backtesting => "Backtesting",
        }
    }
}
