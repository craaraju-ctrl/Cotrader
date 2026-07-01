//! Atr Skills

pub enum AtrSkill {
    Calculation,
    SignalGeneration,
}

impl AtrSkill {
    pub fn name(&self) -> &'static str {
        match self {
            AtrSkill::Calculation => "Calculation",
            AtrSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
