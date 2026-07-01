//! Drawdown Risk
//!
//! Calculates current drawdown from peak equity.
//! Current DD = (peak - current) / peak.
//! DD > 0.10 (10%) = warning.

/// Drawdown Risk evaluator.
///
/// # Examples
/// ```
/// use rat_risk::drawdown::drawdown::DrawdownRisk;
///
/// let risk = DrawdownRisk::new(100_000.0, 90_000.0);
/// assert!((risk.calculate() - 0.10).abs() < 1e-10);
/// assert!(risk.is_warning());
/// ```
pub struct DrawdownRisk {
    /// Peak equity (highest historical portfolio value).
    peak_equity: f64,
    /// Current portfolio equity.
    current_equity: f64,
}

impl DrawdownRisk {
    pub fn name() -> &'static str {
        "DrawdownRisk"
    }

    /// Create a new DrawdownRisk evaluator.
    ///
    /// # Arguments
    /// * `peak_equity` — Highest portfolio value achieved.
    /// * `current_equity` — Current portfolio value.
    pub fn new(peak_equity: f64, current_equity: f64) -> Self {
        Self {
            peak_equity,
            current_equity,
        }
    }

    /// Create from equity curve history. Automatically determines peak.
    ///
    /// # Arguments
    /// * `equity_history` — Time-ordered portfolio values (oldest first).
    pub fn from_history(equity_history: &[f64]) -> Self {
        if equity_history.is_empty() {
            return Self {
                peak_equity: 0.0,
                current_equity: 0.0,
            };
        }

        let peak = equity_history.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let current = *equity_history.last().unwrap();

        Self {
            peak_equity: peak,
            current_equity: current,
        }
    }

    /// Calculate current drawdown from peak.
    ///
    /// Returns a value between 0.0 (at peak) and 1.0 (total loss).
    /// Thresholds:
    /// - < 0.05: Minimal drawdown
    /// - 0.05 – 0.10: Moderate drawdown
    /// - > 0.10: Warning — significant drawdown
    /// - > 0.20: Severe drawdown
    pub fn calculate(&self) -> f64 {
        if self.peak_equity <= 0.0 {
            return 0.0;
        }

        let dd = (self.peak_equity - self.current_equity) / self.peak_equity;
        dd.clamp(0.0, 1.0)
    }

    /// Convenience: is drawdown above the 10% warning threshold?
    pub fn is_warning(&self) -> bool {
        self.calculate() >= 0.10
    }

    /// Convenience: is drawdown in severe territory (>20%)?
    pub fn is_severe(&self) -> bool {
        self.calculate() > 0.20
    }

    /// Maximum drawdown from an equity curve.
    pub fn max_drawdown(equity_history: &[f64]) -> f64 {
        if equity_history.len() < 2 {
            return 0.0;
        }

        let mut peak = equity_history[0];
        let mut max_dd = 0.0;

        for &value in equity_history.iter().skip(1) {
            if value > peak {
                peak = value;
            }
            if peak > 0.0 {
                let dd = (peak - value) / peak;
                if dd > max_dd {
                    max_dd = dd;
                }
            }
        }

        max_dd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_drawdown() {
        let risk = DrawdownRisk::new(100_000.0, 100_000.0);
        assert_eq!(risk.calculate(), 0.0);
        assert!(!risk.is_warning());
    }

    #[test]
    fn test_10_percent_drawdown() {
        let risk = DrawdownRisk::new(100_000.0, 90_000.0);
        assert!((risk.calculate() - 0.10).abs() < 1e-10);
        assert!(risk.is_warning());
        assert!(!risk.is_severe());
    }

    #[test]
    fn test_50_percent_drawdown() {
        let risk = DrawdownRisk::new(200_000.0, 100_000.0);
        assert!((risk.calculate() - 0.50).abs() < 1e-10);
        assert!(risk.is_warning());
        assert!(risk.is_severe());
    }

    #[test]
    fn test_from_history() {
        let history = vec![100.0, 110.0, 105.0, 95.0, 100.0];
        let risk = DrawdownRisk::from_history(&history);
        // Peak = 110, current = 100, DD = 10/110 ≈ 0.0909
        assert!((risk.calculate() - 10.0 / 110.0).abs() < 1e-10);
    }

    #[test]
    fn test_max_drawdown() {
        let history = vec![100.0, 120.0, 90.0, 110.0];
        // Peak goes 100→120, then drops to 90: DD = 30/120 = 0.25
        let max_dd = DrawdownRisk::max_drawdown(&history);
        assert!((max_dd - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_zero_peak() {
        let risk = DrawdownRisk::new(0.0, 100.0);
        assert_eq!(risk.calculate(), 0.0);
    }
}
