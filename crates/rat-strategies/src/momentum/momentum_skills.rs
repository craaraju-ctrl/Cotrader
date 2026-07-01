//! Momentum Skills

pub enum MomentumSkill {
    SignalGeneration,
    Backtesting,
}

impl MomentumSkill {
    pub fn name(&self) -> &'static str {
        match self {
            MomentumSkill::SignalGeneration => "SignalGeneration",
            MomentumSkill::Backtesting => "Backtesting",
        }
    }
}
