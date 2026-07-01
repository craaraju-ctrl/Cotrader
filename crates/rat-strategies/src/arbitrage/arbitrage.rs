//! Arbitrage Strategy

pub struct ArbitrageStrategy;

impl ArbitrageStrategy {
    pub fn name() -> &'static str { "ArbitrageStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
