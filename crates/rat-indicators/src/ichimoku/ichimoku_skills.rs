//! Ichimoku Skills

pub enum IchimokuSkill {
    Calculation,
    SignalGeneration,
}

impl IchimokuSkill {
    pub fn name(&self) -> &'static str {
        match self {
            IchimokuSkill::Calculation => "Calculation",
            IchimokuSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
