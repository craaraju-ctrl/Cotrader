//! Execution Skills

pub enum ExecutionSkill {
    Processing,
    Filtering,
}

impl ExecutionSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ExecutionSkill::Processing => "Processing",
            ExecutionSkill::Filtering => "Filtering",
        }
    }
}
