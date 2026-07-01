//! Zerodha Skills

pub enum ZerodhaSkill {
    OrderPlacement,
    PositionQuery,
    BalanceQuery,
}

impl ZerodhaSkill {
    pub fn name(&self) -> &'static str {
        match self {
            ZerodhaSkill::OrderPlacement => "OrderPlacement",
            ZerodhaSkill::PositionQuery => "PositionQuery",
            ZerodhaSkill::BalanceQuery => "BalanceQuery",
        }
    }
}
