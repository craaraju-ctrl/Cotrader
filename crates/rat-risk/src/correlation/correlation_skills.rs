//! Correlation Skills

pub enum CorrelationSkill {
    Calculation,
    Monitoring,
}

impl CorrelationSkill {
    pub fn name(&self) -> &'static str {
        match self {
            CorrelationSkill::Calculation => "Calculation",
            CorrelationSkill::Monitoring => "Monitoring",
        }
    }
}
