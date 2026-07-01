//! Working Skills

pub enum WorkingSkill {
    Store,
    Retrieve,
    Search,
}

impl WorkingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            WorkingSkill::Store => "Store",
            WorkingSkill::Retrieve => "Retrieve",
            WorkingSkill::Search => "Search",
        }
    }
}
