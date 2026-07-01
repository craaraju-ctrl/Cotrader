//! Alpaca Rules

pub enum AlpacaRule {
    MaxOrdersPerSecond(u32),
    RateLimit(u64),
}

impl AlpacaRule {
    pub fn name(&self) -> &'static str {
        match self {
            AlpacaRule::MaxOrdersPerSecond(_) => "MaxOrdersPerSecond",
            AlpacaRule::RateLimit(_) => "RateLimit",
        }
    }
}
