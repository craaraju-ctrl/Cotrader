//! LowLiquidity Tools

pub enum LowLiquidityTool {
    Detector,
    Classifier,
}

impl LowLiquidityTool {
    pub fn name(&self) -> &'static str {
        match self {
            LowLiquidityTool::Detector => "Detector",
            LowLiquidityTool::Classifier => "Classifier",
        }
    }
}
