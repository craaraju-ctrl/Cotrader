//! LowLiquidity Skills

pub enum LowLiquiditySkill {
    Detection,
    Transition,
}

impl LowLiquiditySkill {
    pub fn name(&self) -> &'static str {
        match self {
            LowLiquiditySkill::Detection => "Detection",
            LowLiquiditySkill::Transition => "Transition",
        }
    }
}
