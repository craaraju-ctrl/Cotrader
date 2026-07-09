//! Cornish-Fisher Value-at-Risk (VaR) computation module.
//!
//! Implements the Cornish-Fisher expansion to adjust the standard normal critical
//! value for non-normal, skewed asset return distributions. This provides dynamic
//! statistical drawdown boundaries that replace static risk checks.
//!
//! # Formula
//!
//! ```text
//! Z_cf = Z_α + (Z_α² - 1) * S/6 + (Z_α³ - 3*Z_α) * K/24 - (2*Z_α³ - 5*Z_α) * S²/36
//!
//! Where:
//!   Z_α = norm.ppf(α) for α = 0.01 (99% confidence)
//!   S = rolling skewness of returns
//!   K = rolling excess kurtosis of returns
//!
//! VaR = -(μ + Z_cf * σ)
//! ```

use serde::{Deserialize, Serialize};

/// Configuration for Cornish-Fisher VaR computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaRConfig {
    /// Confidence level (e.g., 0.99 for 99% VaR).
    pub confidence_level: f64,
    /// Rolling window size in bars for computing skewness/kurtosis.
    pub lookback_window: usize,
    /// Maximum portfolio VaR as fraction of equity (e.g., 0.05 = 5%).
    pub risk_tolerance: f64,
    /// Maximum volatility ratio (current vol / normal vol) before emergency gate.
    pub volatility_cap: f64,
    /// Whether VaR emergency gate is enabled.
    pub enabled: bool,
}

impl Default for VaRConfig {
    fn default() -> Self {
        Self {
            confidence_level: 0.99,
            lookback_window: 60,
            risk_tolerance: 0.05,
            volatility_cap: 3.0,
            enabled: true,
        }
    }
}

/// Result of Cornish-Fisher VaR computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaRResult {
    /// Value-at-Risk at the specified confidence level (as fraction of portfolio).
    pub var_alpha: f64,
    /// Cornish-Fisher adjusted critical value.
    pub z_cf: f64,
    /// Standard normal critical value (Z_α).
    pub z_alpha: f64,
    /// Rolling skewness of returns.
    pub skewness: f64,
    /// Rolling excess kurtosis of returns.
    pub kurtosis: f64,
    /// Annualized volatility (standard deviation of returns).
    pub volatility: f64,
    /// Mean return over the lookback window.
    pub mean_return: f64,
    /// Volatility ratio (current / typical) for emergency gate.
    pub volatility_ratio: f64,
    /// Whether the VaR computation was successful.
    pub valid: bool,
    /// Reason if computation failed.
    pub error: Option<String>,
}

impl Default for VaRResult {
    fn default() -> Self {
        Self {
            var_alpha: 0.0,
            z_cf: 0.0,
            z_alpha: 0.0,
            skewness: 0.0,
            kurtosis: 0.0,
            volatility: 0.0,
            mean_return: 0.0,
            volatility_ratio: 1.0,
            valid: false,
            error: Some("No data provided".to_string()),
        }
    }
}

/// Compute Cornish-Fisher VaR from a slice of closing prices.
///
/// # Arguments
/// * `closes` - Slice of closing prices (must have at least `lookback_window` elements)
/// * `config` - VaR configuration parameters
///
/// # Returns
/// `VaRResult` with the computed VaR and diagnostic information.
pub fn compute_cornish_fisher_var(closes: &[f64], config: &VaRConfig) -> VaRResult {
    if closes.len() < 3 {
        return VaRResult {
            error: Some(format!("Insufficient data: {} closes (need >= 3)", closes.len())),
            ..Default::default()
        };
    }

    // Compute returns from closes
    let returns: Vec<f64> = closes
        .windows(2)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect();

    compute_var_from_returns(&returns, config)
}

/// Compute Cornish-Fisher VaR from a slice of returns.
///
/// # Arguments
/// * `returns` - Slice of period returns (e.g., daily returns)
/// * `config` - VaR configuration parameters
///
/// # Returns
/// `VaRResult` with the computed VaR and diagnostic information.
pub fn compute_var_from_returns(returns: &[f64], config: &VaRConfig) -> VaRResult {
    let n = returns.len();

    if n < 3 {
        return VaRResult {
            error: Some(format!("Insufficient returns: {} (need >= 3)", n)),
            ..Default::default()
        };
    }

    // Use the most recent lookback_window returns (or all if fewer)
    let window_size = n.min(config.lookback_window);
    let window = &returns[n - window_size..];

    // Compute mean
    let mean = window.iter().sum::<f64>() / window_size as f64;

    // Compute variance and standard deviation
    let variance = window.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (window_size - 1) as f64;
    let std_dev = variance.sqrt();

    // Handle zero volatility edge case
    if std_dev < 1e-12 {
        return VaRResult {
            var_alpha: 0.0,
            z_cf: 0.0,
            z_alpha: 0.0,
            skewness: 0.0,
            kurtosis: 0.0,
            volatility: 0.0,
            mean_return: mean,
            volatility_ratio: 0.0,
            valid: true,
            error: None,
        };
    }

    // Compute skewness (Fisher's definition)
    let m3 = window.iter().map(|r| ((r - mean) / std_dev).powi(3)).sum::<f64>() / window_size as f64;
    let skewness = m3;

    // Compute excess kurtosis (Fisher's definition, excess = kurtosis - 3)
    let m4 = window.iter().map(|r| ((r - mean) / std_dev).powi(4)).sum::<f64>() / window_size as f64;
    let excess_kurtosis = m4 - 3.0;

    // Standard normal critical value for the given confidence level
    // For 99% VaR: Z_0.01 = -2.3263 (left tail)
    let alpha = 1.0 - config.confidence_level;
    let z_alpha = normal_ppf(alpha);

    // Cornish-Fisher expansion
    let z2 = z_alpha * z_alpha;
    let z3 = z2 * z_alpha;

    let z_cf = z_alpha
        + (z2 - 1.0) * skewness / 6.0
        + (z3 - 3.0 * z_alpha) * excess_kurtosis / 24.0
        - (2.0 * z3 - 5.0 * z_alpha) * skewness * skewness / 36.0;

    // VaR = -(mean + z_cf * std_dev)
    // Negative sign because VaR is a loss (positive number)
    let var_alpha = -(mean + z_cf * std_dev);

    // Volatility ratio: current std_dev annualized vs typical crypto vol (~1.5% daily)
    // For crypto, typical daily volatility is around 1.5-3%
    let typical_daily_vol = 0.015;
    let volatility_ratio = (std_dev / typical_daily_vol).clamp(0.1, 10.0);

    VaRResult {
        var_alpha,
        z_cf,
        z_alpha,
        skewness,
        kurtosis: excess_kurtosis,
        volatility: std_dev,
        mean_return: mean,
        volatility_ratio,
        valid: true,
        error: None,
    }
}

/// Standard normal inverse CDF (probit function) approximation.
///
/// Uses the rational approximation from Abramowitz and Stegun (1964).
/// Accuracy: |error| < 4.5e-4 for p in (0.00001, 0.99999).
fn normal_ppf(p: f64) -> f64 {
    // Clamp to valid range
    let p = p.clamp(1e-10, 1.0 - 1e-10);

    // For p > 0.5, use symmetry: Z(p) = -Z(1-p)
    if p > 0.5 {
        return -normal_ppf(1.0 - p);
    }

    // Abramowitz and Stegun approximation for p < 0.5
    let t = (-2.0 * p.ln()).sqrt();

    let c0 = 2.515517;
    let c1 = 0.802853;
    let c2 = 0.010328;
    let d1 = 1.432788;
    let d2 = 0.189269;
    let d3 = 0.001308;

    let z = t - (c0 + c1 * t + c2 * t * t) / (1.0 + d1 * t + d2 * t * t + d3 * t * t * t);

    -z // Negative because we're looking for left tail
}

/// Check if VaR exceeds risk tolerance and volatility is within bounds.
///
/// Returns `Some(reason)` if the emergency gate should trigger, `None` otherwise.
pub fn check_var_emergency_gate(
    var_result: &VaRResult,
    config: &VaRConfig,
) -> Option<String> {
    if !config.enabled || !var_result.valid {
        return None;
    }

    // Check 1: VaR exceeds risk tolerance
    if var_result.var_alpha > config.risk_tolerance {
        return Some(format!(
            "VaR emergency: VaR={:.4} exceeds tolerance={:.4} (skew={:.3}, kurt={:.3})",
            var_result.var_alpha, config.risk_tolerance, var_result.skewness, var_result.kurtosis
        ));
    }

    // Check 2: Volatility breaks standard limits
    if var_result.volatility_ratio > config.volatility_cap {
        return Some(format!(
            "Volatility emergency: ratio={:.2}x exceeds cap={:.2}x (daily vol={:.4})",
            var_result.volatility_ratio, config.volatility_cap, var_result.volatility
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_ppf_known_values() {
        // Z(0.05) ≈ -1.645
        let z05 = normal_ppf(0.05);
        assert!((z05 - (-1.645)).abs() < 0.001, "Z(0.05) = {}", z05);

        // Z(0.01) ≈ -2.326
        let z01 = normal_ppf(0.01);
        assert!((z01 - (-2.326)).abs() < 0.001, "Z(0.01) = {}", z01);

        // Z(0.5) = 0
        let z50 = normal_ppf(0.5);
        assert!(z50.abs() < 0.001, "Z(0.5) = {}", z50);
    }

    #[test]
    fn test_zero_volatility() {
        let closes = vec![100.0; 10]; // No price change
        let config = VaRConfig::default();
        let result = compute_cornish_fisher_var(&closes, &config);

        assert!(result.valid);
        assert_eq!(result.var_alpha, 0.0);
        assert_eq!(result.volatility, 0.0);
    }

    #[test]
    fn test_normal_returns() {
        // Generate normal-ish returns with some volatility
        let closes: Vec<f64> = (0..100)
            .map(|i| 100.0 * (1.0 + 0.001 * ((i as f64 - 50.0) / 50.0).sin()))
            .collect();
        let config = VaRConfig::default();
        let result = compute_cornish_fisher_var(&closes, &config);

        assert!(result.valid);
        // VaR can be positive or negative depending on the returns
        // For this test, we just verify the computation is valid
        assert!(result.volatility > 0.0);
    }

    #[test]
    fn test_skewed_returns() {
        // Create positively skewed returns (more big wins than big losses)
        let mut closes = vec![100.0];
        for i in 1..100 {
            let return_pct = if i % 10 == 0 { 0.05 } else { -0.001 };
            closes.push(closes[i - 1] * (1.0 + return_pct));
        }
        let config = VaRConfig::default();
        let result = compute_cornish_fisher_var(&closes, &config);

        assert!(result.valid);
        assert!(result.skewness > 0.0, "Should be positively skewed");
    }

    #[test]
    fn test_emergency_gate_triggered() {
        let result = VaRResult {
            var_alpha: 0.10, // 10% VaR
            valid: true,
            ..Default::default()
        };
        let config = VaRConfig {
            risk_tolerance: 0.05, // 5% tolerance
            ..Default::default()
        };

        let emergency = check_var_emergency_gate(&result, &config);
        assert!(emergency.is_some(), "Should trigger emergency gate");
    }

    #[test]
    fn test_emergency_gate_not_triggered() {
        let result = VaRResult {
            var_alpha: 0.03, // 3% VaR
            volatility_ratio: 1.5,
            valid: true,
            ..Default::default()
        };
        let config = VaRConfig {
            risk_tolerance: 0.05,
            volatility_cap: 3.0,
            ..Default::default()
        };

        let emergency = check_var_emergency_gate(&result, &config);
        assert!(emergency.is_none(), "Should not trigger emergency gate");
    }

    #[test]
    fn test_insufficient_data() {
        let closes = vec![100.0, 101.0]; // Only 2 data points
        let config = VaRConfig::default();
        let result = compute_cornish_fisher_var(&closes, &config);

        assert!(!result.valid);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_volatility_cap_triggered() {
        let result = VaRResult {
            var_alpha: 0.02, // Low VaR
            volatility_ratio: 4.0, // But high vol
            valid: true,
            ..Default::default()
        };
        let config = VaRConfig {
            risk_tolerance: 0.05,
            volatility_cap: 3.0,
            ..Default::default()
        };

        let emergency = check_var_emergency_gate(&result, &config);
        assert!(emergency.is_some(), "Should trigger on volatility cap");
    }
}
