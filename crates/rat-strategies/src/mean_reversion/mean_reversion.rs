//! MeanReversion Strategy

pub struct MeanReversionStrategy;

impl MeanReversionStrategy {
    pub fn name() -> &'static str { "MeanReversionStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
