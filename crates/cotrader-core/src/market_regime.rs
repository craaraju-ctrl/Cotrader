//! MarketRegime — Shared market regime enum used across crates.
//!
//! Defined here to avoid circular dependencies between rat-ml and rat-autonomous.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketRegime {
    TrendingBull,
    TrendingBear,
    Ranging,
    Volatile,
    LowLiquidity,
}

impl std::fmt::Display for MarketRegime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TrendingBull => write!(f, "TrendingBull"),
            Self::TrendingBear => write!(f, "TrendingBear"),
            Self::Ranging => write!(f, "Ranging"),
            Self::Volatile => write!(f, "Volatile"),
            Self::LowLiquidity => write!(f, "LowLiquidity"),
        }
    }
}
