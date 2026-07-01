//! Volatility Risk
//!
//! Calculates if current volatility exceeds normal range.
//! Uses Average True Range (ATR) relative to historical ATR average.
//! Current ATR > 1.5x historical average = high volatility risk.

/// Volatility Risk evaluator.
///
/// # Examples
/// ```
/// use rat_risk::volatility::volatility::VolatilityRisk;
///
/// let risk = VolatilityRisk::new(2.5, 1.5); // current ATR, avg ATR
/// assert!(risk.calculate() > 0.0); // 2.5/1.5 = 1.67x → high vol
/// ```
pub struct VolatilityRisk {
    /// Current ATR (Average True Range) value.
    current_atr: f64,
    /// Historical average ATR (rolling mean).
    avg_atr: f64,
}

impl VolatilityRisk {
    pub fn name() -> &'static str {
        "VolatilityRisk"
    }

    /// Create a new VolatilityRisk evaluator with pre-computed ATR values.
    ///
    /// # Arguments
    /// * `current_atr` — Current ATR value.
    /// * `avg_atr` — Historical average ATR (e.g. 20-period rolling mean).
    pub fn new(current_atr: f64, avg_atr: f64) -> Self {
        Self {
            current_atr,
            avg_atr,
        }
    }

    /// Create from OHLC price data. Computes ATR over the given period.
    ///
    /// # Arguments
    /// * `highs` — Series of high prices.
    /// * `lows` — Series of low prices.
    /// * `closes` — Series of close prices.
    /// * `atr_period` — ATR lookback period (e.g. 14).
    /// * `avg_period` — How many recent ATR values to average for baseline.
    pub fn from_ohlc(
        highs: &[f64],
        lows: &[f64],
        closes: &[f64],
        atr_period: usize,
        avg_period: usize,
    ) -> Self {
        let true_ranges = Self::compute_true_ranges(highs, lows, closes);
        let atrs = Self::compute_atrs(&true_ranges, atr_period);

        if atrs.is_empty() {
            return Self {
                current_atr: 0.0,
                avg_atr: 0.0,
            };
        }

        let current_atr = *atrs.last().unwrap();

        // Average of the last `avg_period` ATR values (excluding current)
        let lookback = avg_period.min(atrs.len().saturating_sub(1));
        let avg_atr = if lookback > 0 {
            atrs[atrs.len() - 1 - lookback..atrs.len() - 1]
                .iter()
                .sum::<f64>()
                / lookback as f64
        } else {
            current_atr
        };

        Self {
            current_atr,
            avg_atr,
        }
    }

    /// Calculate volatility risk score.
    ///
    /// Returns 0.0 (normal volatility) to 1.0 (extreme volatility).
    ///
    /// Based on ATR ratio:
    /// - ratio < 1.0: Below average volatility (low risk)
    /// - ratio 1.0 – 1.5: Normal range
    /// - ratio > 1.5: High volatility risk
    /// - ratio > 2.0: Extreme volatility
    pub fn calculate(&self) -> f64 {
        if self.avg_atr <= 0.0 {
            return 0.0;
        }

        let ratio = self.current_atr / self.avg_atr;

        // Risk scales linearly from 1.0x (risk=0) to 2.0x (risk=1.0)
        if ratio <= 1.0 {
            0.0
        } else {
            ((ratio - 1.0)).min(1.0)
        }
    }

    /// ATR ratio (current / historical average).
    pub fn atr_ratio(&self) -> f64 {
        if self.avg_atr <= 0.0 {
            0.0
        } else {
            self.current_atr / self.avg_atr
        }
    }

    /// Convenience: is volatility above the 1.5x high-risk threshold?
    pub fn is_high_risk(&self) -> bool {
        self.atr_ratio() > 1.5
    }

    /// Convenience: is volatility in extreme territory (>2.0x)?
    pub fn is_extreme(&self) -> bool {
        self.atr_ratio() > 2.0
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// Compute True Range series from OHLC data.
    fn compute_true_ranges(highs: &[f64], lows: &[f64], closes: &[f64]) -> Vec<f64> {
        let len = highs.len().min(lows.len()).min(closes.len());
        if len < 2 {
            return vec![];
        }

        let mut trs = Vec::with_capacity(len - 1);
        for i in 1..len {
            let hl = highs[i] - lows[i];
            let hc = (highs[i] - closes[i - 1]).abs();
            let lc = (lows[i] - closes[i - 1]).abs();
            trs.push(hl.max(hc).max(lc));
        }
        trs
    }

    /// Compute ATR series using Wilder's smoothing (exponential moving average).
    fn compute_atrs(true_ranges: &[f64], period: usize) -> Vec<f64> {
        if true_ranges.len() < period {
            return vec![];
        }

        let mut atrs = Vec::with_capacity(true_ranges.len() - period + 1);

        // First ATR is simple average of first `period` true ranges
        let first_atr: f64 = true_ranges[..period].iter().sum::<f64>() / period as f64;
        atrs.push(first_atr);

        // Subsequent ATRs use Wilder's smoothing: ATR = (prev_ATR * (period-1) + TR) / period
        let mut prev_atr = first_atr;
        for &tr in &true_ranges[period..] {
            let atr = (prev_atr * (period as f64 - 1.0) + tr) / period as f64;
            atrs.push(atr);
            prev_atr = atr;
        }

        atrs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_volatility() {
        let risk = VolatilityRisk::new(1.0, 1.0);
        assert_eq!(risk.calculate(), 0.0);
        assert!(!risk.is_high_risk());
    }

    #[test]
    fn test_high_volatility() {
        // ATR 1.5x average → right at threshold
        let risk = VolatilityRisk::new(1.5, 1.0);
        assert!(!risk.is_high_risk()); // exactly 1.5x, not > 1.5
        assert!((risk.calculate() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_extreme_volatility() {
        let risk = VolatilityRisk::new(3.0, 1.0);
        assert!(risk.is_extreme());
        assert!(risk.is_high_risk());
        assert!((risk.calculate() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_below_average() {
        let risk = VolatilityRisk::new(0.5, 1.0);
        assert_eq!(risk.calculate(), 0.0);
    }

    #[test]
    fn test_from_ohlc() {
        // Generate some OHLC data with increasing volatility
        let n = 30;
        let highs: Vec<f64> = (0..n).map(|i| 100.0 + (i as f64 * 0.5)).collect();
        let lows: Vec<f64> = (0..n).map(|i| 99.0 - (i as f64 * 0.3)).collect();
        let closes: Vec<f64> = (0..n)
            .map(|i| 99.5 + (i as f64 * 0.1))
            .collect();

        let risk = VolatilityRisk::from_ohlc(&highs, &lows, &closes, 14, 10);
        assert!(risk.current_atr > 0.0);
        assert!(risk.avg_atr > 0.0);
    }

    #[test]
    fn test_zero_avg_atr() {
        let risk = VolatilityRisk::new(1.0, 0.0);
        assert_eq!(risk.calculate(), 0.0);
    }
}
