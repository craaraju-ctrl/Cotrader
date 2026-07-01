//! Bollinger Skills

pub enum BollingerSkill {
    Calculation,
    SignalGeneration,
}

impl BollingerSkill {
    pub fn name(&self) -> &'static str {
        match self {
            BollingerSkill::Calculation => "Calculation",
            BollingerSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
