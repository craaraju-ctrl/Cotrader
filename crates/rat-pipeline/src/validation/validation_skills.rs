//! Validation Skills

pub enum ValidationSkill {
    Processing,
    Filtering,
}

impl ValidationSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ValidationSkill::Processing => "Processing",
            ValidationSkill::Filtering => "Filtering",
        }
    }
}
