//! SignalGeneration Skills

pub enum SignalGenerationSkill {
    Processing,
    Filtering,
}

impl SignalGenerationSkill {
    pub fn name(&self) -> &'static str {
        match self {
            SignalGenerationSkill::Processing => "Processing",
            SignalGenerationSkill::Filtering => "Filtering",
        }
    }
}
