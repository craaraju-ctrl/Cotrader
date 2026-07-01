//! Cci Skills

pub enum CciSkill {
    Calculation,
    SignalGeneration,
}

impl CciSkill {
    pub fn name(&self) -> &'static str {
        match self {
            CciSkill::Calculation => "Calculation",
            CciSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
