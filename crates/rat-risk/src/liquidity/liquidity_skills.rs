//! Liquidity Skills

pub enum LiquiditySkill {
    Calculation,
    Monitoring,
}

impl LiquiditySkill {
    pub fn name(&self) -> &'static str {
        match self {
            LiquiditySkill::Calculation => "Calculation",
            LiquiditySkill::Monitoring => "Monitoring",
        }
    }
}
