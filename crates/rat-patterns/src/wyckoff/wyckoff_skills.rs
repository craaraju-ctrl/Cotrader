//! Wyckoff Skills

pub enum WyckoffSkill {
    Detection,
    Confirmation,
}

impl WyckoffSkill {
    pub fn name(&self) -> &'static str {
        match self {
            WyckoffSkill::Detection => "Detection",
            WyckoffSkill::Confirmation => "Confirmation",
        }
    }
}
