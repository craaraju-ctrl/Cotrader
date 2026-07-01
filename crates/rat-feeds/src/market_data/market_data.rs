//! MarketData Feed

pub struct MarketDataFeed;

impl MarketDataFeed {
    pub fn name() -> &'static str { "MarketDataFeed" }
    pub fn fetch(&self) -> Vec<String> { vec![] }
}
