//! Volatile Regime

pub struct VolatileRegime;

impl VolatileRegime {
    pub fn name() -> &'static str { "VolatileRegime" }
    pub fn detect(&self) -> bool { false }
}
