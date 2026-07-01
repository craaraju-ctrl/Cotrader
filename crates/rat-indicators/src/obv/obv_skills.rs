//! Obv Skills

pub enum ObvSkill {
    Calculation,
    SignalGeneration,
}

impl ObvSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ObvSkill::Calculation => "Calculation",
            ObvSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
