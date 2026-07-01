//! Pairs Skills

pub enum PairsSkill {
    SignalGeneration,
    Backtesting,
}

impl PairsSkill {
    pub fn name(&self) -> &'static str {
        match self {
            PairsSkill::SignalGeneration => "SignalGeneration",
            PairsSkill::Backtesting => "Backtesting",
        }
    }
}
