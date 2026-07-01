//! Ranging Skills

pub enum RangingSkill {
    Detection,
    Transition,
}

impl RangingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            RangingSkill::Detection => "Detection",
            RangingSkill::Transition => "Transition",
        }
    }
}
