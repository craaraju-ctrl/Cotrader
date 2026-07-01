//! Angelone Rules

pub enum AngeloneRule {
    MaxOrdersPerSecond(u32),
    RateLimit(u64),
}

impl AngeloneRule {
    pub fn name(&self) -> &'static str {
        match self {
            AngeloneRule::MaxOrdersPerSecond(_) => "MaxOrdersPerSecond",
            AngeloneRule::RateLimit(_) => "RateLimit",
        }
    }
}
