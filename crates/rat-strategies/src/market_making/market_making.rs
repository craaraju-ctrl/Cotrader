//! MarketMaking Strategy

pub struct MarketMakingStrategy;

impl MarketMakingStrategy {
    pub fn name() -> &'static str { "MarketMakingStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
