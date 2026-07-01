//! Volatility Skills

pub enum VolatilitySkill {
    Calculation,
    Monitoring,
}

impl VolatilitySkill {
    pub fn name(&self) -> &'static str {
        match self {
            VolatilitySkill::Calculation => "Calculation",
            VolatilitySkill::Monitoring => "Monitoring",
        }
    }
}
