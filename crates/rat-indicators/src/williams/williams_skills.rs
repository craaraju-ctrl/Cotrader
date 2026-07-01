//! Williams Skills

pub enum WilliamsSkill {
    Calculation,
    SignalGeneration,
}

impl WilliamsSkill {
    pub fn name(&self) -> &'static str {
        match self {
            WilliamsSkill::Calculation => "Calculation",
            WilliamsSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
