//! NewsFeed Feed

pub struct NewsFeedFeed;

impl NewsFeedFeed {
    pub fn name() -> &'static str { "NewsFeedFeed" }
    pub fn fetch(&self) -> Vec<String> { vec![] }
}
