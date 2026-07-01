//! Arbitrage Skills

pub enum ArbitrageSkill {
    SignalGeneration,
    Backtesting,
}

impl ArbitrageSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ArbitrageSkill::SignalGeneration => "SignalGeneration",
            ArbitrageSkill::Backtesting => "Backtesting",
        }
    }
}
