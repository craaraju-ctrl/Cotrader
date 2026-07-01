//! Swing Strategy

pub struct SwingStrategy;

impl SwingStrategy {
    pub fn name() -> &'static str { "SwingStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
