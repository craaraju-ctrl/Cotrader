//! Donchian Skills

pub enum DonchianSkill {
    Calculation,
    SignalGeneration,
}

impl DonchianSkill {
    pub fn name(&self) -> &'static str {
        match self {
            DonchianSkill::Calculation => "Calculation",
            DonchianSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
