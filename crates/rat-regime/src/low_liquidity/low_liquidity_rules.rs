//! LowLiquidity Rules

pub enum LowLiquidityRule {
    MinConfidence(f64),
    MaxDuration(u64),
}

impl LowLiquidityRule {
    pub fn name(&self) -> &'static str {
        match self {
            LowLiquidityRule::MinConfidence(_) => "MinConfidence",
            LowLiquidityRule::MaxDuration(_) => "MaxDuration",
        }
    }
}
