//! Breakout Skills

pub enum BreakoutSkill {
    SignalGeneration,
    Backtesting,
}

impl BreakoutSkill {
    pub fn name(&self) -> &'static str {
        match self {
            BreakoutSkill::SignalGeneration => "SignalGeneration",
            BreakoutSkill::Backtesting => "Backtesting",
        }
    }
}
