//! Exchange Skills

pub enum ExchangeSkill {
    OrderPlacement,
    PositionQuery,
    BalanceQuery,
}

impl ExchangeSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ExchangeSkill::OrderPlacement => "OrderPlacement",
            ExchangeSkill::PositionQuery => "PositionQuery",
            ExchangeSkill::BalanceQuery => "BalanceQuery",
        }
    }
}
