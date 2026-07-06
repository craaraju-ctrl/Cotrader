//! Asset Class — Classifies trading instruments for per-asset processing.
//!
//! Different asset classes require different:
//! - Indicators (RSI works everywhere, Greeks for options, swap rates for forex)
//! - Risk parameters (2% for crypto, 5% for equities, 10% for forex)
//! - Market hours (24/7 for crypto, market hours for equities)
//! - Order types (MIS/CNC for India, IOC/GTC for crypto)

use serde::{Deserialize, Serialize};
use std::fmt;

/// Asset class with exchange-specific metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssetClass {
    /// Equity stock with exchange info
    Equity {
        symbol: String,
        exchange: String, // "NYSE", "NASDAQ", "NSE", "BSE", "TSE", "LSE"
    },
    /// Cryptocurrency
    Crypto {
        symbol: String, // "BTC", "ETH", "SOL"
    },
    /// Foreign exchange pair
    Forex {
        pair: String, // "EUR/USD", "GBP/JPY"
    },
    /// Commodity (spot or futures)
    Commodity {
        symbol: String, // "XAU/USD", "CL" (crude oil)
        is_futures: bool,
    },
    /// Futures contract
    Future {
        symbol: String, // "ES" (S&P e-mini), "NIFTY"
        expiry: String, // "2026-09"
    },
    /// Option contract
    Option {
        underlying: Box<AssetClass>,
        strike: f64,
        expiry: String,
        option_type: OptionType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    Call,
    Put,
}

impl AssetClass {
    /// Get the base currency for this asset.
    pub fn currency(&self) -> &str {
        match self {
            AssetClass::Equity { exchange, .. } => match exchange.as_str() {
                "NSE" | "BSE" => "INR",
                "TSE" => "JPY",
                _ => "USD",
            },
            AssetClass::Crypto { .. } => "USD",
            AssetClass::Forex { pair } => {
                // First currency in pair is the base
                pair.split('/').next().unwrap_or("USD")
            }
            AssetClass::Commodity { .. } => "USD",
            AssetClass::Future { .. } => "USD",
            AssetClass::Option { underlying, .. } => underlying.currency(),
        }
    }

    /// Get the asset class category for risk management.
    pub fn category(&self) -> AssetCategory {
        match self {
            AssetClass::Equity { .. } => AssetCategory::Equity,
            AssetClass::Crypto { .. } => AssetCategory::Crypto,
            AssetClass::Forex { .. } => AssetCategory::Forex,
            AssetClass::Commodity { .. } => AssetCategory::Commodity,
            AssetClass::Future { .. } => AssetCategory::Derivative,
            AssetClass::Option { .. } => AssetCategory::Derivative,
        }
    }

    /// Get recommended max risk per trade for this asset class.
    pub fn max_risk_per_trade(&self) -> f64 {
        match self.category() {
            AssetCategory::Equity => 0.05,    // 5% max risk
            AssetCategory::Crypto => 0.02,    // 2% max risk
            AssetCategory::Forex => 0.03,     // 3% max risk
            AssetCategory::Commodity => 0.03, // 3% max risk
            AssetCategory::Derivative => 0.02, // 2% max risk (leverage)
        }
    }

    /// Check if this asset trades 24/7.
    pub fn is_24_7(&self) -> bool {
        matches!(self, AssetClass::Crypto { .. })
    }
}

/// High-level asset categories for risk aggregation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetCategory {
    Equity,
    Crypto,
    Forex,
    Commodity,
    Derivative,
}

impl fmt::Display for AssetClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetClass::Equity { symbol, exchange } => write!(f, "{} ({})", symbol, exchange),
            AssetClass::Crypto { symbol } => write!(f, "{} (Crypto)", symbol),
            AssetClass::Forex { pair } => write!(f, "{} (Forex)", pair),
            AssetClass::Commodity { symbol, is_futures } => {
                if *is_futures {
                    write!(f, "{} (Futures)", symbol)
                } else {
                    write!(f, "{} (Spot)", symbol)
                }
            }
            AssetClass::Future { symbol, expiry } => write!(f, "{} {} (Futures)", symbol, expiry),
            AssetClass::Option { underlying, strike, expiry, option_type } => {
                write!(f, "{:?} {} {} {}", option_type, underlying, strike, expiry)
            }
        }
    }
}
