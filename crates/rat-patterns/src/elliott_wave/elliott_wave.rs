//! ElliottWave Patterns

pub struct ElliottWaveDetector;

impl ElliottWaveDetector {
    pub fn name() -> &'static str { "ElliottWaveDetector" }
    pub fn detect(&self) -> Vec<String> { vec![] }
}
