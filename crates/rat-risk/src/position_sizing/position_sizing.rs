//! Position Sizing Risk
//!
//! Calculates if a position exceeds the max allowed % of portfolio.
//! Uses Kelly Criterion: f* = (bp - q) / b
//! where b = win/loss ratio, p = win probability, q = 1 - p.

/// Position Sizing Risk evaluator.
///
/// # Examples
/// ```
/// use rat_risk::position_sizing::position_sizing::PositionSizingRisk;
///
/// // Position: $5000 in a $100000 portfolio, Kelly says max 10%
/// let risk = PositionSizingRisk::new(5000.0, 100_000.0, 0.55, 2.0, 0.10);
/// assert_eq!(risk.calculate(), 0.0); // 5% < 10% max → safe
/// ```
pub struct PositionSizingRisk {
    /// Dollar value of the current position.
    position_value: f64,
    /// Total portfolio value.
    portfolio_value: f64,
    /// Historical win probability (0.0 to 1.0).
    win_probability: f64,
    /// Win/loss ratio (average win / average loss).
    win_loss_ratio: f64,
    /// Maximum allowed fraction of portfolio (e.g. 0.10 for 10%).
    max_allowed_fraction: f64,
}

impl PositionSizingRisk {
    pub fn name() -> &'static str {
        "PositionSizingRisk"
    }

    /// Create a new PositionSizingRisk evaluator.
    ///
    /// # Arguments
    /// * `position_value` — Current dollar value of the position.
    /// * `portfolio_value` — Total portfolio equity.
    /// * `win_probability` — Historical win rate (0.0 to 1.0).
    /// * `win_loss_ratio` — Average win / average loss (e.g. 2.0 means wins are 2x losses).
    /// * `max_allowed_fraction` — Max fraction of portfolio for this position (e.g. 0.10).
    pub fn new(
        position_value: f64,
        portfolio_value: f64,
        win_probability: f64,
        win_loss_ratio: f64,
        max_allowed_fraction: f64,
    ) -> Self {
        Self {
            position_value,
            portfolio_value,
            win_probability,
            win_loss_ratio,
            max_allowed_fraction,
        }
    }

    /// Calculate position sizing risk score.
    ///
    /// Returns 0.0 (within limits) to 1.0 (severely oversized).
    ///
    /// Two components:
    /// 1. **Kelly fraction**: f* = (b*p - q) / b. If current weight > f*, risk is high.
    /// 2. **Max allowed fraction**: If current weight > max_allowed, risk is high.
    ///
    /// The score is based on how much the current position exceeds the tighter
    /// of the Kelly-optimal and max-allowed limits.
    pub fn calculate(&self) -> f64 {
        if self.portfolio_value <= 0.0 {
            return 0.0;
        }

        let current_fraction = self.position_value / self.portfolio_value;

        // Kelly criterion: f* = (b*p - q) / b
        let kelly_fraction = self.kelly_fraction();

        // Use the tighter constraint
        let limit = if kelly_fraction > 0.0 {
            kelly_fraction.min(self.max_allowed_fraction)
        } else {
            // No edge (Kelly says don't bet) — use max allowed
            self.max_allowed_fraction
        };

        if limit <= 0.0 {
            return if current_fraction > 0.0 { 1.0 } else { 0.0 };
        }

        // Risk = how much we exceed the limit, normalized
        let excess = current_fraction - limit;
        if excess <= 0.0 {
            0.0
        } else {
            // 10% over limit → 0.5 risk, 20%+ over → 1.0
            (excess / limit).min(1.0)
        }
    }

    /// Calculate Kelly fraction: f* = (b*p - q) / b
    ///
    /// Returns 0.0 when edge is negative (no bet).
    /// Capped at 0.25 (25% of portfolio) for safety.
    pub fn kelly_fraction(&self) -> f64 {
        let p = self.win_probability.clamp(0.0, 1.0);
        let b = self.win_loss_ratio.max(0.01);
        let q = 1.0 - p;

        let edge = p * b - q;
        if edge > 0.0 {
            (edge / b).clamp(0.0, 0.25)
        } else {
            0.0
        }
    }

    /// Current position weight as fraction of portfolio.
    pub fn current_fraction(&self) -> f64 {
        if self.portfolio_value <= 0.0 {
            0.0
        } else {
            self.position_value / self.portfolio_value
        }
    }

    /// Convenience: is the position oversized relative to Kelly?
    pub fn exceeds_kelly(&self) -> bool {
        self.current_fraction() > self.kelly_fraction()
    }

    /// Convenience: does the position exceed the max allowed fraction?
    pub fn exceeds_max_allowed(&self) -> bool {
        self.current_fraction() > self.max_allowed_fraction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_within_limits() {
        // $5k in $100k = 5%, Kelly says ~10%, max allowed 10% → safe
        let risk = PositionSizingRisk::new(5000.0, 100_000.0, 0.55, 2.0, 0.10);
        assert_eq!(risk.calculate(), 0.0);
        assert!(!risk.exceeds_kelly());
        assert!(!risk.exceeds_max_allowed());
    }

    #[test]
    fn test_exceeds_kelly() {
        // Kelly: p=0.51, b=1.1 → f* ≈ 6.5%. Position at 15% exceeds it.
        let risk = PositionSizingRisk::new(15_000.0, 100_000.0, 0.51, 1.1, 0.25);
        assert!(risk.calculate() > 0.0);
        assert!(risk.exceeds_kelly());
    }

    #[test]
    fn test_exceeds_max() {
        // $30k in $100k = 30%, max allowed 10%
        let risk = PositionSizingRisk::new(30_000.0, 100_000.0, 0.55, 2.0, 0.10);
        assert!(risk.calculate() > 0.0);
        assert!(risk.exceeds_max_allowed());
    }

    #[test]
    fn test_negative_edge() {
        // 40% win rate, 1:1 payoff → Kelly says don't bet
        let risk = PositionSizingRisk::new(5000.0, 100_000.0, 0.40, 1.0, 0.10);
        assert_eq!(risk.kelly_fraction(), 0.0);
        // Still has position → risk if > max allowed
        assert!(!risk.exceeds_max_allowed()); // 5% < 10%
    }

    #[test]
    fn test_kelly_formula() {
        // 60% win rate, 1.5:1 payoff
        // f* = (1.5*0.6 - 0.4) / 1.5 = (0.9 - 0.4) / 1.5 = 0.333, capped at 0.25
        let risk = PositionSizingRisk::new(1000.0, 10_000.0, 0.60, 1.5, 0.50);
        let kelly = risk.kelly_fraction();
        assert!((kelly - 0.25).abs() < 0.01, "Expected ~0.25, got {}", kelly);
    }

    #[test]
    fn test_zero_portfolio() {
        let risk = PositionSizingRisk::new(5000.0, 0.0, 0.55, 2.0, 0.10);
        assert_eq!(risk.calculate(), 0.0);
    }
}
