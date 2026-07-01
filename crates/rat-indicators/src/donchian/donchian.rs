//! Donchian Indicator

pub struct DonchianIndicator;

impl DonchianIndicator {
    pub fn name() -> &'static str { "DonchianIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
