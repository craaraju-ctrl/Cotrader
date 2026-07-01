//! Ichimoku Indicator

pub struct IchimokuIndicator;

impl IchimokuIndicator {
    pub fn name() -> &'static str { "IchimokuIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
