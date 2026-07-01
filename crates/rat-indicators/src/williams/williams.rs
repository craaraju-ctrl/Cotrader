//! Williams Indicator

pub struct WilliamsIndicator;

impl WilliamsIndicator {
    pub fn name() -> &'static str { "WilliamsIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
