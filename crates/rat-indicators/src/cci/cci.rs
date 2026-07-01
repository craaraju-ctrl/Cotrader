//! Cci Indicator

pub struct CciIndicator;

impl CciIndicator {
    pub fn name() -> &'static str { "CciIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
