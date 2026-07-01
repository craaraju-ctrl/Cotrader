//! Graph Skills

pub enum GraphSkill {
    Store,
    Retrieve,
    Search,
}

impl GraphSkill {
    pub fn name(&self) -> &'static str {
        match self {
            GraphSkill::Store => "Store",
            GraphSkill::Retrieve => "Retrieve",
            GraphSkill::Search => "Search",
        }
    }
}
