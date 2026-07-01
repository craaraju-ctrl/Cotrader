//! Ranging Regime

pub struct RangingRegime;

impl RangingRegime {
    pub fn name() -> &'static str { "RangingRegime" }
    pub fn detect(&self) -> bool { false }
}
