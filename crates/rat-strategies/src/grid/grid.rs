//! Grid Strategy

pub struct GridStrategy;

impl GridStrategy {
    pub fn name() -> &'static str { "GridStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
