//! Drawdown Skills

pub enum DrawdownSkill {
    Calculation,
    Monitoring,
}

impl DrawdownSkill {
    pub fn name(&self) -> &'static str {
        match self {
            DrawdownSkill::Calculation => "Calculation",
            DrawdownSkill::Monitoring => "Monitoring",
        }
    }
}
