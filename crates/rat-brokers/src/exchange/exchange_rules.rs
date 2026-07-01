//! Exchange Rules

pub enum ExchangeRule {
    MaxOrdersPerSecond(u32),
    RateLimit(u64),
}

impl ExchangeRule {
    pub fn name(&self) -> &'static str {
        match self {
            ExchangeRule::MaxOrdersPerSecond(_) => "MaxOrdersPerSecond",
            ExchangeRule::RateLimit(_) => "RateLimit",
        }
    }
}
