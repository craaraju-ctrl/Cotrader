//! Obv Indicator

pub struct ObvIndicator;

impl ObvIndicator {
    pub fn name() -> &'static str { "ObvIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
