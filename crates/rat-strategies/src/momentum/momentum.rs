//! Momentum Strategy

pub struct MomentumStrategy;

impl MomentumStrategy {
    pub fn name() -> &'static str { "MomentumStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
