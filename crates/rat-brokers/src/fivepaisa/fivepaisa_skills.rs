//! Fivepaisa Skills

pub enum FivepaisaSkill {
    OrderPlacement,
    PositionQuery,
    BalanceQuery,
}

impl FivepaisaSkill {
    pub fn name(&self) -> &'static str {
        match self {
            FivepaisaSkill::OrderPlacement => "OrderPlacement",
            FivepaisaSkill::PositionQuery => "PositionQuery",
            FivepaisaSkill::BalanceQuery => "BalanceQuery",
        }
    }
}
