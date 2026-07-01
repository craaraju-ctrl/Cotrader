//! Volatile Skills

pub enum VolatileSkill {
    Detection,
    Transition,
}

impl VolatileSkill {
    pub fn name(&self) -> &'static str {
        match self {
            VolatileSkill::Detection => "Detection",
            VolatileSkill::Transition => "Transition",
        }
    }
}
