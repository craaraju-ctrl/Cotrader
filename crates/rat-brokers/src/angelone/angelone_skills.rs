//! Angelone Skills

pub enum AngeloneSkill {
    OrderPlacement,
    PositionQuery,
    BalanceQuery,
}

impl AngeloneSkill {
    pub fn name(&self) -> &'static str {
        match self {
            AngeloneSkill::OrderPlacement => "OrderPlacement",
            AngeloneSkill::PositionQuery => "PositionQuery",
            AngeloneSkill::BalanceQuery => "BalanceQuery",
        }
    }
}
