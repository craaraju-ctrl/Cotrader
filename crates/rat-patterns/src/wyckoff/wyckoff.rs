//! Wyckoff Patterns

pub struct WyckoffDetector;

impl WyckoffDetector {
    pub fn name() -> &'static str { "WyckoffDetector" }
    pub fn detect(&self) -> Vec<String> { vec![] }
}
