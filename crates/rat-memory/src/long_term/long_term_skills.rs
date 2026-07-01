//! LongTerm Skills

pub enum LongTermSkill {
    Store,
    Retrieve,
    Search,
}

impl LongTermSkill {
    pub fn name(&self) -> &'static str {
        match self {
            LongTermSkill::Store => "Store",
            LongTermSkill::Retrieve => "Retrieve",
            LongTermSkill::Search => "Search",
        }
    }
}
