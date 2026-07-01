//! Trending Regime

pub struct TrendingRegime;

impl TrendingRegime {
    pub fn name() -> &'static str { "TrendingRegime" }
    pub fn detect(&self) -> bool { false }
}
