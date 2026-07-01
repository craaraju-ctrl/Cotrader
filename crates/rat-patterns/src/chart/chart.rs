//! Chart Patterns

pub struct ChartDetector;

impl ChartDetector {
    pub fn name() -> &'static str { "ChartDetector" }
    pub fn detect(&self) -> Vec<String> { vec![] }
}
