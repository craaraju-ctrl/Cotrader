//! Volatility Risk

pub struct VolatilityRisk;

impl VolatilityRisk {
    pub fn name() -> &'static str { "VolatilityRisk" }
    pub fn calculate(&self) -> f64 { 0.0 }
}
