//! Swing Skills

pub enum SwingSkill {
    SignalGeneration,
    Backtesting,
}

impl SwingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            SwingSkill::SignalGeneration => "SignalGeneration",
            SwingSkill::Backtesting => "Backtesting",
        }
    }
}
