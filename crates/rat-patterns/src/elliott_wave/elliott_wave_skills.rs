//! ElliottWave Skills

pub enum ElliottWaveSkill {
    Detection,
    Confirmation,
}

impl ElliottWaveSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ElliottWaveSkill::Detection => "Detection",
            ElliottWaveSkill::Confirmation => "Confirmation",
        }
    }
}
