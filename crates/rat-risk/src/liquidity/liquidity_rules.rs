//! Liquidity Rules

pub enum LiquidityRule {
    MaxThreshold(f64),
    WarningLevel(f64),
}

impl LiquidityRule {
    pub fn name(&self) -> &'static str {
        match self {
            LiquidityRule::MaxThreshold(_) => "MaxThreshold",
            LiquidityRule::WarningLevel(_) => "WarningLevel",
        }
    }
}
