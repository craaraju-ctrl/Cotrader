//! Pivot Indicator

pub struct PivotIndicator;

impl PivotIndicator {
    pub fn name() -> &'static str { "PivotIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
