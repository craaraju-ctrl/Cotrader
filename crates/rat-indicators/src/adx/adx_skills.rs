//! Adx Skills

pub enum AdxSkill {
    Calculation,
    SignalGeneration,
}

impl AdxSkill {
    pub fn name(&self) -> &'static str {
        match self {
            AdxSkill::Calculation => "Calculation",
            AdxSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
