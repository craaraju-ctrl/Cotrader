//! Chart Skills

pub enum ChartSkill {
    Detection,
    Confirmation,
}

impl ChartSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ChartSkill::Detection => "Detection",
            ChartSkill::Confirmation => "Confirmation",
        }
    }
}
