//! Volume Indicator

pub struct VolumeIndicator;

impl VolumeIndicator {
    pub fn name() -> &'static str { "VolumeIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
