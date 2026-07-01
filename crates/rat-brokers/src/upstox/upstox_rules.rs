//! Upstox Rules

pub enum UpstoxRule {
    MaxOrdersPerSecond(u32),
    RateLimit(u64),
}

impl UpstoxRule {
    pub fn name(&self) -> &'static str {
        match self {
            UpstoxRule::MaxOrdersPerSecond(_) => "MaxOrdersPerSecond",
            UpstoxRule::RateLimit(_) => "RateLimit",
        }
    }
}
