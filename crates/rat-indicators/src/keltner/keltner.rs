//! Keltner Indicator

pub struct KeltnerIndicator;

impl KeltnerIndicator {
    pub fn name() -> &'static str { "KeltnerIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
