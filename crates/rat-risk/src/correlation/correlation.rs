//! Correlation Risk
//!
//! Calculates average pairwise Pearson correlation across portfolio assets.
//! Average correlation > 0.7 = high correlation risk (positions move together).

/// Correlation Risk evaluator.
///
/// # Examples
/// ```
/// use rat_risk::correlation::correlation::CorrelationRisk;
///
/// // Two assets with some return series
/// let prices_a = vec![100.0, 102.0, 101.0, 105.0, 104.0];
/// let prices_b = vec![200.0, 204.0, 202.0, 210.0, 208.0];
/// let risk = CorrelationRisk::new(vec![prices_a, prices_b]);
/// assert!(risk.calculate() > 0.9); // Highly correlated
/// ```
pub struct CorrelationRisk {
    /// Price series for each asset (each inner vec = time-series of prices).
    price_series: Vec<Vec<f64>>,
}

impl CorrelationRisk {
    pub fn name() -> &'static str {
        "CorrelationRisk"
    }

    /// Create a new CorrelationRisk evaluator.
    ///
    /// # Arguments
    /// * `price_series` — Vector of price series, one per asset. Each series
    ///   should have the same length (same time period).
    pub fn new(price_series: Vec<Vec<f64>>) -> Self {
        Self { price_series }
    }

    /// Calculate average pairwise Pearson correlation across all asset pairs.
    ///
    /// Returns a value between -1.0 and 1.0.
    /// Thresholds:
    /// - < 0.3: Low correlation (good diversification)
    /// - 0.3 – 0.7: Moderate correlation
    /// - > 0.7: High correlation risk
    pub fn calculate(&self) -> f64 {
        let n = self.price_series.len();
        if n < 2 {
            return 0.0;
        }

        // Convert prices to returns for each asset
        let returns: Vec<Vec<f64>> = self
            .price_series
            .iter()
            .map(|prices| Self::prices_to_returns(prices))
            .collect();

        // Compute average pairwise Pearson correlation
        let mut total_corr = 0.0;
        let mut count = 0;

        for i in 0..n {
            for j in (i + 1)..n {
                if let Some(corr) = Self::pearson_correlation(&returns[i], &returns[j]) {
                    total_corr += corr;
                    count += 1;
                }
            }
        }

        if count == 0 {
            0.0
        } else {
            total_corr / count as f64
        }
    }

    /// Convenience: is the portfolio above the high-correlation threshold?
    pub fn is_high_risk(&self) -> bool {
        self.calculate() > 0.7
    }

    /// Convert price series to simple returns: r_t = (p_t - p_{t-1}) / p_{t-1}
    fn prices_to_returns(prices: &[f64]) -> Vec<f64> {
        prices
            .windows(2)
            .filter_map(|w| {
                if w[0] != 0.0 {
                    Some((w[1] - w[0]) / w[0])
                } else {
                    None
                }
            })
            .collect()
    }

    /// Compute Pearson correlation coefficient between two return series.
    fn pearson_correlation(x: &[f64], y: &[f64]) -> Option<f64> {
        let len = x.len().min(y.len());
        if len < 3 {
            return None; // Need at least 3 data points
        }

        let x = &x[..len];
        let y = &y[..len];

        let mean_x: f64 = x.iter().sum::<f64>() / len as f64;
        let mean_y: f64 = y.iter().sum::<f64>() / len as f64;

        let mut cov = 0.0;
        let mut var_x = 0.0;
        let mut var_y = 0.0;

        for i in 0..len {
            let dx = x[i] - mean_x;
            let dy = y[i] - mean_y;
            cov += dx * dy;
            var_x += dx * dx;
            var_y += dy * dy;
        }

        let denom = (var_x * var_y).sqrt();
        if denom < 1e-15 {
            return Some(0.0); // No variance → uncorrelated
        }

        Some(cov / denom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfectly_correlated() {
        // Varying returns that are perfectly proportional
        let a = vec![100.0, 110.0, 105.0, 115.0, 108.0];
        let b = vec![200.0, 220.0, 210.0, 230.0, 216.0]; // b = 2*a exactly
        let risk = CorrelationRisk::new(vec![a, b]);
        let corr = risk.calculate();
        assert!((corr - 1.0).abs() < 1e-10, "Expected ~1.0, got {}", corr);
    }

    #[test]
    fn test_uncorrelated() {
        // Construct two series with zero correlation
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let b = vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
        let risk = CorrelationRisk::new(vec![a, b]);
        let corr = risk.calculate();
        assert!(corr.abs() < 0.5, "Expected low correlation, got {}", corr);
    }

    #[test]
    fn test_single_asset() {
        let risk = CorrelationRisk::new(vec![vec![1.0, 2.0, 3.0]]);
        assert_eq!(risk.calculate(), 0.0);
    }

    #[test]
    fn test_empty() {
        let risk = CorrelationRisk::new(vec![]);
        assert_eq!(risk.calculate(), 0.0);
    }

    #[test]
    fn test_too_few_data_points() {
        let a = vec![100.0, 110.0];
        let b = vec![200.0, 220.0];
        let risk = CorrelationRisk::new(vec![a, b]);
        // Only 1 return point — not enough for Pearson
        assert_eq!(risk.calculate(), 0.0);
    }
}
