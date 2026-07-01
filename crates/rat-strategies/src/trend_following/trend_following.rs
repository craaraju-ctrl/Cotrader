//! TrendFollowing Strategy

pub struct TrendFollowingStrategy;

impl TrendFollowingStrategy {
    pub fn name() -> &'static str { "TrendFollowingStrategy" }
    pub fn generate_signal(&self) -> String { "HOLD".to_string() }
}
