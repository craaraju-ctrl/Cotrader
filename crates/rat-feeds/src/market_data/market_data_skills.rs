//! MarketData Skills

pub enum MarketDataSkill {
    Fetch,
    Parse,
}

impl MarketDataSkill {
    pub fn name(&self) -> &'static str {
        match self {
            MarketDataSkill::Fetch => "Fetch",
            MarketDataSkill::Parse => "Parse",
        }
    }
}
