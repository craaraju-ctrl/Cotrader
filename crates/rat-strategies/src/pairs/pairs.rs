//! Pairs Strategy

pub struct PairsStrategy;

impl PairsStrategy {
    pub fn name() -> &'static str { "PairsStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
