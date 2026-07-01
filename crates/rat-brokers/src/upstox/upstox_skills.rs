//! Upstox Skills

pub enum UpstoxSkill {
    OrderPlacement,
    PositionQuery,
    BalanceQuery,
}

impl UpstoxSkill {
    pub fn name(&self) -> &'static str {
        match self {
            UpstoxSkill::OrderPlacement => "OrderPlacement",
            UpstoxSkill::PositionQuery => "PositionQuery",
            UpstoxSkill::BalanceQuery => "BalanceQuery",
        }
    }
}
