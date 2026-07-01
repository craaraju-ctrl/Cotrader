//! Fivepaisa Rules

pub enum FivepaisaRule {
    MaxOrdersPerSecond(u32),
    RateLimit(u64),
}

impl FivepaisaRule {
    pub fn name(&self) -> &'static str {
        match self {
            FivepaisaRule::MaxOrdersPerSecond(_) => "MaxOrdersPerSecond",
            FivepaisaRule::RateLimit(_) => "RateLimit",
        }
    }
}
