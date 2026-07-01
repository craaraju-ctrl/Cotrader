//! Volume Skills

pub enum VolumeSkill {
    Calculation,
    SignalGeneration,
}

impl VolumeSkill {
    pub fn name(&self) -> &'static str {
        match self {
            VolumeSkill::Calculation => "Calculation",
            VolumeSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
