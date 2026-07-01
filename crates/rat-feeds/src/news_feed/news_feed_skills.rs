//! NewsFeed Skills

pub enum NewsFeedSkill {
    Fetch,
    Parse,
}

impl NewsFeedSkill {
    pub fn name(&self) -> &'static str {
        match self {
            NewsFeedSkill::Fetch => "Fetch",
            NewsFeedSkill::Parse => "Parse",
        }
    }
}
