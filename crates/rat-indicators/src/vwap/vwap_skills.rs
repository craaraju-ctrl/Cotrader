//! Vwap Skills

pub enum VwapSkill {
    Calculation,
    SignalGeneration,
}

impl VwapSkill {
    pub fn name(&self) -> &'static str {
        match self {
            VwapSkill::Calculation => "Calculation",
            VwapSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
