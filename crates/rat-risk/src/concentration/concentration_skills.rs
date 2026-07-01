//! Concentration Skills

pub enum ConcentrationSkill {
    Calculation,
    Monitoring,
}

impl ConcentrationSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ConcentrationSkill::Calculation => "Calculation",
            ConcentrationSkill::Monitoring => "Monitoring",
        }
    }
}
