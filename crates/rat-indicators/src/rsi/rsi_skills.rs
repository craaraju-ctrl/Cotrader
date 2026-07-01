//! Rsi Skills

pub enum RsiSkill {
    Calculation,
    SignalGeneration,
}

impl RsiSkill {
    pub fn name(&self) -> &'static str {
        match self {
            RsiSkill::Calculation => "Calculation",
            RsiSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
