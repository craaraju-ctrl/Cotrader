//! Transition Skills

pub enum TransitionSkill {
    Detection,
    Transition,
}

impl TransitionSkill {
    pub fn name(&self) -> &'static str {
        match self {
            TransitionSkill::Detection => "Detection",
            TransitionSkill::Transition => "Transition",
        }
    }
}
