//! Binance Rules

pub enum BinanceRule {
    MaxOrdersPerSecond(u32),
    RateLimit(u64),
}

impl BinanceRule {
    pub fn name(&self) -> &'static str {
        match self {
            BinanceRule::MaxOrdersPerSecond(_) => "MaxOrdersPerSecond",
            BinanceRule::RateLimit(_) => "RateLimit",
        }
    }
}
