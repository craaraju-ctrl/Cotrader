//! Keltner Skills

pub enum KeltnerSkill {
    Calculation,
    SignalGeneration,
}

impl KeltnerSkill {
    pub fn name(&self) -> &'static str {
        match self {
            KeltnerSkill::Calculation => "Calculation",
            KeltnerSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
