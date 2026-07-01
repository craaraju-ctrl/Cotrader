//! Postprocessing Skills

pub enum PostprocessingSkill {
    Processing,
    Filtering,
}

impl PostprocessingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            PostprocessingSkill::Processing => "Processing",
            PostprocessingSkill::Filtering => "Filtering",
        }
    }
}
