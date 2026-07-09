//! Symbol normalization — the single source of truth for symbol format handling.
//!
//! # The Problem
//!
//! Different layers of the system use different symbol formats:
//! - Watchlist / Portfolio: bare symbols (`"BTC"`, `"ETH"`)
//! - Tredo Exchange API: paired symbols (`"BTC/USD"`, `"ETH/USD"`)
//! - Binance API: concatenated symbols (`"BTCUSDT"`, `"ETHUSDT"`)
//!
//! Comparing raw strings across these formats fails silently, causing:
//! - Duplicate order placement (position check fails)
//! - P&L updates not finding open positions
//! - Pipeline skipping symbols that have open positions
//!
//! # The Solution
//!
//! `SymbolPair` normalizes any input format to a canonical bare form for
//! internal comparisons, while providing conversion methods for external APIs.
//!
//! All internal storage (watchlist, portfolio, signals) uses bare symbols.
//! All external API calls use `to_tredo()` or `to_binance()`.

use std::fmt;

/// Canonical symbol representation for the trading system.
///
/// Internally always stored as the bare form (`"BTC"`, `"ETH"`).
/// Provides lossless conversion to/from all external formats.
///
/// # Examples
///
/// ```
/// use cotrader_core::symbol::SymbolPair;
///
/// // All of these normalize to "BTC"
/// assert_eq!(SymbolPair::new("BTC").as_str(), "BTC");
/// assert_eq!(SymbolPair::new("BTC/USD").as_str(), "BTC");
/// assert_eq!(SymbolPair::new("BTCUSDT").as_str(), "BTC");
///
/// // Conversions for external APIs
/// assert_eq!(SymbolPair::new("BTC").to_tredo(), "BTC/USD");
/// assert_eq!(SymbolPair::new("BTC").to_binance(), "BTCUSDT");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolPair {
    bare: String,
}

impl SymbolPair {
    /// Create a new SymbolPair from any format (bare, paired, or concatenated).
    pub fn new(raw: &str) -> Self {
        Self {
            bare: Self::normalize_bare(raw),
        }
    }

    /// Create from a known bare symbol (skips normalization).
    /// Use when you already have a validated bare symbol.
    pub fn from_bare(bare: String) -> Self {
        Self { bare }
    }

    /// Get the bare symbol (e.g., `"BTC"`).
    pub fn as_str(&self) -> &str {
        &self.bare
    }

    /// Get the bare symbol, consuming self.
    pub fn into_inner(self) -> String {
        self.bare
    }

    /// Convert to Tredo Exchange format: `"BTC/USD"`.
    pub fn to_tredo(&self) -> String {
        if self.bare.contains('/') {
            self.bare.clone()
        } else {
            format!("{}/USD", self.bare)
        }
    }

    /// Convert to Binance format: `"BTCUSDT"`.
    pub fn to_binance(&self) -> String {
        format!("{}USDT", self.bare)
    }

    /// Check if this is a crypto symbol (vs equity/forex).
    pub fn is_crypto(&self) -> bool {
        crate::is_crypto_symbol(&self.bare)
    }

    /// Strip common suffixes to get the bare form.
    /// "BTC/USD" → "BTC", "BTCUSDT" → "BTC", "ETH" → "ETH"
    fn normalize_bare(raw: &str) -> String {
        let s = raw.trim().to_uppercase();

        // Already bare
        if !s.contains('/') && !s.ends_with("USDT") && !s.ends_with("USD") {
            return s;
        }

        // Paired format: "BTC/USD" → "BTC"
        if let Some(base) = s.strip_suffix("/USD") {
            return base.to_string();
        }
        if let Some(base) = s.strip_suffix("/USDT") {
            return base.to_string();
        }

        // Concatenated format: "BTCUSDT" → "BTC"
        if let Some(base) = s.strip_suffix("USDT") {
            return base.to_string();
        }
        if let Some(base) = s.strip_suffix("USD") {
            return base.to_string();
        }

        s
    }
}

impl fmt::Display for SymbolPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.bare)
    }
}

impl AsRef<str> for SymbolPair {
    fn as_ref(&self) -> &str {
        &self.bare
    }
}

impl From<&str> for SymbolPair {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for SymbolPair {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

/// Build a HashSet of bare symbols from a slice of any-format symbols.
/// Useful for `open_symbols.contains(symbol)` checks.
pub fn bare_symbol_set(symbols: &[impl AsRef<str>]) -> std::collections::HashSet<String> {
    symbols
        .iter()
        .map(|s| SymbolPair::new(s.as_ref()).into_inner())
        .collect()
}

/// Check if two symbol strings match after normalization.
/// "BTC" == "BTC/USD" == "BTCUSDT" → true
pub fn symbols_match(a: &str, b: &str) -> bool {
    SymbolPair::new(a) == SymbolPair::new(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_bare() {
        assert_eq!(SymbolPair::new("BTC").as_str(), "BTC");
        assert_eq!(SymbolPair::new("btc").as_str(), "BTC");
        assert_eq!(SymbolPair::new("BTC/USD").as_str(), "BTC");
        assert_eq!(SymbolPair::new("btc/usd").as_str(), "BTC");
        assert_eq!(SymbolPair::new("BTCUSDT").as_str(), "BTC");
        assert_eq!(SymbolPair::new("btcusdt").as_str(), "BTC");
        assert_eq!(SymbolPair::new("ETH/USD").as_str(), "ETH");
        assert_eq!(SymbolPair::new("ETHUSDT").as_str(), "ETH");
        assert_eq!(SymbolPair::new("SOL").as_str(), "SOL");
    }

    #[test]
    fn test_to_tredo() {
        assert_eq!(SymbolPair::new("BTC").to_tredo(), "BTC/USD");
        assert_eq!(SymbolPair::new("BTC/USD").to_tredo(), "BTC/USD");
        assert_eq!(SymbolPair::new("BTCUSDT").to_tredo(), "BTC/USD");
    }

    #[test]
    fn test_to_binance() {
        assert_eq!(SymbolPair::new("BTC").to_binance(), "BTCUSDT");
        assert_eq!(SymbolPair::new("BTC/USD").to_binance(), "BTCUSDT");
        assert_eq!(SymbolPair::new("BTCUSDT").to_binance(), "BTCUSDT");
    }

    #[test]
    fn test_symbols_match() {
        assert!(symbols_match("BTC", "BTC"));
        assert!(symbols_match("BTC", "BTC/USD"));
        assert!(symbols_match("BTC", "BTCUSDT"));
        assert!(symbols_match("BTC/USD", "BTCUSDT"));
        assert!(symbols_match("btc", "BTC"));
        assert!(symbols_match("btc/usd", "BTC"));
        assert!(!symbols_match("BTC", "ETH"));
        assert!(!symbols_match("BTC", "ETH/USD"));
    }

    #[test]
    fn test_bare_symbol_set() {
        let set = bare_symbol_set(&["BTC", "BTC/USD", "ETHUSDT", "SOL"]);
        assert!(set.contains("BTC"));
        assert!(set.contains("ETH"));
        assert!(set.contains("SOL"));
        assert_eq!(set.len(), 3); // BTC deduplicated
    }

    #[test]
    fn test_equality() {
        assert_eq!(SymbolPair::new("BTC"), SymbolPair::new("BTC/USD"));
        assert_eq!(SymbolPair::new("BTC"), SymbolPair::new("BTCUSDT"));
        assert_ne!(SymbolPair::new("BTC"), SymbolPair::new("ETH"));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", SymbolPair::new("BTC/USD")), "BTC");
    }

    #[test]
    fn test_from_traits() {
        let s: SymbolPair = "BTC/USD".into();
        assert_eq!(s.as_str(), "BTC");
        let s: SymbolPair = "BTCUSDT".to_string().into();
        assert_eq!(s.as_str(), "BTC");
    }
}
