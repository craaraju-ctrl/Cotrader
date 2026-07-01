//! Episodic Skills

pub enum EpisodicSkill {
    Store,
    Retrieve,
    Search,
}

impl EpisodicSkill {
    pub fn name(&self) -> &'static str {
        match self {
            EpisodicSkill::Store => "Store",
            EpisodicSkill::Retrieve => "Retrieve",
            EpisodicSkill::Search => "Search",
        }
    }
}
