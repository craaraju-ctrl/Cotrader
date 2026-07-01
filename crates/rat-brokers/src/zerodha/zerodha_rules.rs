//! Zerodha Rules

pub enum ZerodhaRule {
    MaxOrdersPerSecond(u32),
    RateLimit(u64),
}

impl ZerodhaRule {
    pub fn name(&self) -> &'static str {
        match self {
            ZerodhaRule::MaxOrdersPerSecond(_) => "MaxOrdersPerSecond",
            ZerodhaRule::RateLimit(_) => "RateLimit",
        }
    }
}
