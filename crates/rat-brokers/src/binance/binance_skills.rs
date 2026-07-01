//! Binance Skills

pub enum BinanceSkill {
    OrderPlacement,
    PositionQuery,
    BalanceQuery,
}

impl BinanceSkill {
    pub fn name(&self) -> &'static str {
        match self {
            BinanceSkill::OrderPlacement => "OrderPlacement",
            BinanceSkill::PositionQuery => "PositionQuery",
            BinanceSkill::BalanceQuery => "BalanceQuery",
        }
    }
}
