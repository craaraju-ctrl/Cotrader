//! Concentration Risk
//!
//! Calculates Herfindahl-Hirschman Index (HHI) across portfolio positions.
//! HHI = sum(z_i^2) where z_i = position weight.
//! HHI > 0.25 = high concentration risk.

/// Concentration Risk evaluator using HHI.
///
/// # Examples
/// ```
/// use rat_risk::concentration::concentration::ConcentrationRisk;
///
/// let risk = ConcentrationRisk::new(vec![0.5, 0.3, 0.2]);
/// assert!(risk.calculate() > 0.0);
/// ```
pub struct ConcentrationRisk {
    /// Position weights that sum to 1.0 (e.g. [0.5, 0.3, 0.2])
    weights: Vec<f64>,
}

impl ConcentrationRisk {
    pub fn name() -> &'static str {
        "ConcentrationRisk"
    }

    /// Create a new ConcentrationRisk evaluator.
    ///
    /// # Arguments
    /// * `weights` — Portfolio position weights (should sum to ~1.0).
    ///   Empty or zero-weight portfolio returns 0.0.
    pub fn new(weights: Vec<f64>) -> Self {
        Self { weights }
    }

    /// Calculate the Herfindahl-Hirschman Index (HHI).
    ///
    /// Returns a value between 0.0 (perfect diversification) and 1.0 (fully concentrated).
    /// Thresholds:
    /// - < 0.15: Low concentration (well diversified)
    /// - 0.15 – 0.25: Moderate concentration
    /// - > 0.25: High concentration risk
    pub fn calculate(&self) -> f64 {
        if self.weights.is_empty() {
            return 0.0;
        }

        // Normalize weights so they sum to 1.0
        let total: f64 = self.weights.iter().sum();
        if total <= 0.0 {
            return 0.0;
        }

        self.weights
            .iter()
            .map(|&w| {
                let normalized = w / total;
                normalized * normalized
            })
            .sum()
    }

    /// Convenience: is the portfolio above the high-concentration threshold?
    pub fn is_high_risk(&self) -> bool {
        self.calculate() > 0.25
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_position() {
        let risk = ConcentrationRisk::new(vec![1.0]);
        assert!((risk.calculate() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_equal_weights() {
        // 4 equal positions: HHI = 4 * (0.25)^2 = 0.25
        let risk = ConcentrationRisk::new(vec![1.0, 1.0, 1.0, 1.0]);
        assert!((risk.calculate() - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_concentrated() {
        // 90% in one stock: HHI = 0.81 + 0.01 * 9 = 0.9
        let mut weights = vec![0.9];
        weights.extend(std::iter::repeat(0.01).take(9));
        let risk = ConcentrationRisk::new(weights);
        assert!(risk.is_high_risk());
        assert!(risk.calculate() > 0.25);
    }

    #[test]
    fn test_diversified() {
        // 10 equal positions: HHI = 10 * (0.1)^2 = 0.1
        let risk = ConcentrationRisk::new(vec![1.0; 10]);
        assert!(!risk.is_high_risk());
    }

    #[test]
    fn test_empty() {
        let risk = ConcentrationRisk::new(vec![]);
        assert_eq!(risk.calculate(), 0.0);
    }

    #[test]
    fn test_zero_weights() {
        let risk = ConcentrationRisk::new(vec![0.0, 0.0]);
        assert_eq!(risk.calculate(), 0.0);
    }
}
