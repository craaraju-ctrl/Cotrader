//! Preprocessing Skills

pub enum PreprocessingSkill {
    Processing,
    Filtering,
}

impl PreprocessingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            PreprocessingSkill::Processing => "Processing",
            PreprocessingSkill::Filtering => "Filtering",
        }
    }
}
