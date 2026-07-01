//! PositionSizing Skills

pub enum PositionSizingSkill {
    Calculation,
    Monitoring,
}

impl PositionSizingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            PositionSizingSkill::Calculation => "Calculation",
            PositionSizingSkill::Monitoring => "Monitoring",
        }
    }
}
