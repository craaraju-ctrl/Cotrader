//! Liquidity Risk

pub struct LiquidityRisk;

impl LiquidityRisk {
    pub fn name() -> &'static str { "LiquidityRisk" }
    pub fn calculate(&self) -> f64 { 0.0 }
}
