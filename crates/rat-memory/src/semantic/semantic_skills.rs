//! Semantic Skills

pub enum SemanticSkill {
    Store,
    Retrieve,
    Search,
}

impl SemanticSkill {
    pub fn name(&self) -> &'static str {
        match self {
            SemanticSkill::Store => "Store",
            SemanticSkill::Retrieve => "Retrieve",
            SemanticSkill::Search => "Search",
        }
    }
}
