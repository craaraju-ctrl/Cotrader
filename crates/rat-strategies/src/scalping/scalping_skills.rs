//! Scalping Skills

pub enum ScalpingSkill {
    SignalGeneration,
    Backtesting,
}

impl ScalpingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ScalpingSkill::SignalGeneration => "SignalGeneration",
            ScalpingSkill::Backtesting => "Backtesting",
        }
    }
}
