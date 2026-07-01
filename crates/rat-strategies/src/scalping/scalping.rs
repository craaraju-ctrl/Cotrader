//! Scalping Strategy

pub struct ScalpingStrategy;

impl ScalpingStrategy {
    pub fn name() -> &'static str { "ScalpingStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
