//! Vwap Indicator

pub struct VwapIndicator;

impl VwapIndicator {
    pub fn name() -> &'static str { "VwapIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
