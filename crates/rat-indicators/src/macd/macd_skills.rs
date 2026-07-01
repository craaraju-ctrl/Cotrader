//! Macd Skills

pub enum MacdSkill {
    Calculation,
    SignalGeneration,
}

impl MacdSkill {
    pub fn name(&self) -> &'static str {
        match self {
            MacdSkill::Calculation => "Calculation",
            MacdSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
