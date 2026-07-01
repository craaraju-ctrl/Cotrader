//! Pivot Skills

pub enum PivotSkill {
    Calculation,
    SignalGeneration,
}

impl PivotSkill {
    pub fn name(&self) -> &'static str {
        match self {
            PivotSkill::Calculation => "Calculation",
            PivotSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
