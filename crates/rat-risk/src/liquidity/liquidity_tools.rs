//! Liquidity Tools

pub enum LiquidityTool {
    Calculator,
    Monitor,
}

impl LiquidityTool {
    pub fn name(&self) -> &'static str {
        match self {
            LiquidityTool::Calculator => "Calculator",
            LiquidityTool::Monitor => "Monitor",
        }
    }
}
