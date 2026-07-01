//! LowLiquidity Regime

pub struct LowLiquidityRegime;

impl LowLiquidityRegime {
    pub fn name() -> &'static str { "LowLiquidityRegime" }
    pub fn detect(&self) -> bool { false }
}
