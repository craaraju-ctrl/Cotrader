//! Alpaca Skills

pub enum AlpacaSkill {
    OrderPlacement,
    PositionQuery,
    BalanceQuery,
}

impl AlpacaSkill {
    pub fn name(&self) -> &'static str {
        match self {
            AlpacaSkill::OrderPlacement => "OrderPlacement",
            AlpacaSkill::PositionQuery => "PositionQuery",
            AlpacaSkill::BalanceQuery => "BalanceQuery",
        }
    }
}
