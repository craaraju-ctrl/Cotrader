//! OnChain Feed

pub struct OnChainFeed;

impl OnChainFeed {
    pub fn name() -> &'static str { "OnChainFeed" }
    pub fn fetch(&self) -> Vec<String> { vec![] }
}
