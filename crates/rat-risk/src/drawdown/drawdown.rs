//! Drawdown Risk

pub struct DrawdownRisk;

impl DrawdownRisk {
    pub fn name() -> &'static str { "DrawdownRisk" }
    pub fn calculate(&self) -> f64 { 0.0 }
}
