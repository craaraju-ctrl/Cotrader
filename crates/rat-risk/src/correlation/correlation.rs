//! Correlation Risk

pub struct CorrelationRisk;

impl CorrelationRisk {
    pub fn name() -> &'static str { "CorrelationRisk" }
    pub fn calculate(&self) -> f64 { 0.0 }
}
