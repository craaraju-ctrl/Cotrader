//! Adx Indicator

pub struct AdxIndicator;

impl AdxIndicator {
    pub fn name() -> &'static str { "AdxIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
