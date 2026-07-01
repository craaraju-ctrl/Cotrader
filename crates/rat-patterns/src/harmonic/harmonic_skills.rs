//! Harmonic Skills

pub enum HarmonicSkill {
    Detection,
    Confirmation,
}

impl HarmonicSkill {
    pub fn name(&self) -> &'static str {
        match self {
            HarmonicSkill::Detection => "Detection",
            HarmonicSkill::Confirmation => "Confirmation",
        }
    }
}
