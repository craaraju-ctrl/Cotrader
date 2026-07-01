//! Volatility Tools

pub enum VolatilityTool {
    Calculator,
    Monitor,
}

impl VolatilityTool {
    pub fn name(&self) -> &'static str {
        match self {
            VolatilityTool::Calculator => "Calculator",
            VolatilityTool::Monitor => "Monitor",
        }
    }
}
