//! NewsFeed Rules

pub enum NewsFeedRule {
    MaxAge(u64),
    MinRelevance(f64),
}

impl NewsFeedRule {
    pub fn name(&self) -> &'static str {
        match self {
            NewsFeedRule::MaxAge(_) => "MaxAge",
            NewsFeedRule::MinRelevance(_) => "MinRelevance",
        }
    }
}
