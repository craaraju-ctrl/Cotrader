//! Breakout Strategy

pub struct BreakoutStrategy;

impl BreakoutStrategy {
    pub fn name() -> &'static str { "BreakoutStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
