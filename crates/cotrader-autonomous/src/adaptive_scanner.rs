//! # Adaptive Scanner
//!
//! Adjusts scan frequency based on market regime:
//! - BLACK_SWAN: 6 seconds (maximum vigilance)
//! - TRENDING_BULL/BEAR: 30 seconds (ride the trend)
//! - MEAN_REVERTING: 45 seconds (catch reversals)
//! - CHOPPY: 60 seconds (reduce noise trading)
//!
//! Replaces the fixed 60-second polling loop with regime-aware timing.

use std::time::{Duration, Instant};

/// Market regime (matches the regime detector's output)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanRegime {
    BlackSwan,
    TrendingBull,
    TrendingBear,
    MeanReverting,
    Choppy,
}

impl ScanRegime {
    /// Scan interval for this regime
    pub fn interval(&self) -> Duration {
        match self {
            ScanRegime::BlackSwan => Duration::from_secs(6),
            ScanRegime::TrendingBull | ScanRegime::TrendingBear => Duration::from_secs(30),
            ScanRegime::MeanReverting => Duration::from_secs(45),
            ScanRegime::Choppy => Duration::from_secs(60),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ScanRegime::BlackSwan => "BLACK_SWAN",
            ScanRegime::TrendingBull => "TRENDING_BULL",
            ScanRegime::TrendingBear => "TRENDING_BEAR",
            ScanRegime::MeanReverting => "MEAN_REVERTING",
            ScanRegime::Choppy => "CHOPPY",
        }
    }
}

/// Tracks per-symbol scan timing with adaptive intervals
pub struct AdaptiveScanner {
    /// Last scan time per symbol
    last_scan: std::collections::HashMap<String, Instant>,
    /// Current regime per symbol
    regimes: std::collections::HashMap<String, ScanRegime>,
    /// Default regime for unknown symbols
    default_regime: ScanRegime,
}

impl Default for AdaptiveScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl AdaptiveScanner {
    pub fn new() -> Self {
        Self {
            last_scan: std::collections::HashMap::new(),
            regimes: std::collections::HashMap::new(),
            default_regime: ScanRegime::Choppy,
        }
    }

    /// Update the regime for a symbol (called when RegimeDetector produces new output)
    pub fn update_regime(&mut self, symbol: &str, regime: ScanRegime) {
        self.regimes.insert(symbol.to_string(), regime);
    }

    /// Check if a symbol is due for scanning
    pub fn should_scan(&self, symbol: &str) -> bool {
        let regime = self.regimes.get(symbol).unwrap_or(&self.default_regime);
        let interval = regime.interval();

        match self.last_scan.get(symbol) {
            Some(last) => last.elapsed() >= interval,
            None => true, // Never scanned — scan now
        }
    }

    /// Mark a symbol as scanned (call after processing)
    pub fn mark_scanned(&mut self, symbol: &str) {
        self.last_scan.insert(symbol.to_string(), Instant::now());
    }

    /// Get the current scan interval for a symbol
    pub fn current_interval(&self, symbol: &str) -> Duration {
        let regime = self.regimes.get(symbol).unwrap_or(&self.default_regime);
        regime.interval()
    }

    /// Get the current regime for a symbol
    pub fn current_regime(&self, symbol: &str) -> ScanRegime {
        self.regimes.get(symbol).copied().unwrap_or(self.default_regime)
    }

    /// Rank symbols by urgency (how overdue they are for scanning)
    /// Returns symbols sorted by overdue time (most overdue first)
    pub fn urgency_ranking(&self, symbols: &[String]) -> Vec<(String, Duration)> {
        let mut ranked: Vec<(String, Duration)> = symbols
            .iter()
            .filter_map(|s| {
                let regime = self.regimes.get(s).unwrap_or(&self.default_regime);
                let interval = regime.interval();
                let overdue = match self.last_scan.get(s) {
                    Some(last) => {
                        let elapsed = last.elapsed();
                        if elapsed > interval { elapsed - interval } else { Duration::ZERO }
                    }
                    None => Duration::from_secs(u64::MAX), // Never scanned — highest urgency
                };
                Some((s.clone(), overdue))
            })
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        ranked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regime_intervals() {
        assert_eq!(ScanRegime::BlackSwan.interval(), Duration::from_secs(6));
        assert_eq!(ScanRegime::TrendingBull.interval(), Duration::from_secs(30));
        assert_eq!(ScanRegime::Choppy.interval(), Duration::from_secs(60));
    }

    #[test]
    fn test_should_scan_first_time() {
        let scanner = AdaptiveScanner::new();
        assert!(scanner.should_scan("BTC"), "Should scan on first access");
    }

    #[test]
    fn test_mark_scanned_prevents_immediate_rescan() {
        let mut scanner = AdaptiveScanner::new();
        scanner.mark_scanned("BTC");
        // With default CHOPPY (60s), should NOT scan again immediately
        assert!(!scanner.should_scan("BTC"));
    }

    #[test]
    fn test_update_regime_changes_interval() {
        let mut scanner = AdaptiveScanner::new();
        scanner.update_regime("BTC", ScanRegime::BlackSwan);
        assert_eq!(scanner.current_regime("BTC"), ScanRegime::BlackSwan);
        assert_eq!(scanner.current_interval("BTC"), Duration::from_secs(6));
    }

    #[test]
    fn test_urgency_ranking() {
        let mut scanner = AdaptiveScanner::new();
        scanner.update_regime("BTC", ScanRegime::BlackSwan);
        scanner.update_regime("ETH", ScanRegime::Choppy);
        scanner.mark_scanned("BTC");
        scanner.mark_scanned("ETH");

        let ranking = scanner.urgency_ranking(&["BTC".to_string(), "ETH".to_string()]);
        assert_eq!(ranking.len(), 2);
        // Both should be 0 overdue since we just scanned
        assert_eq!(ranking[0].1, Duration::ZERO);
    }
}
