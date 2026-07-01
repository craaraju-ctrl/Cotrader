//! Grid Skills

pub enum GridSkill {
    SignalGeneration,
    Backtesting,
}

impl GridSkill {
    pub fn name(&self) -> &'static str {
        match self {
            GridSkill::SignalGeneration => "SignalGeneration",
            GridSkill::Backtesting => "Backtesting",
        }
    }
}
