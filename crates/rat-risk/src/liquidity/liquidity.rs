//! Liquidity Risk
//!
//! Checks if a position can be liquidated within N bars without >1% slippage.
//! Uses volume and order book depth data to estimate execution quality.

/// Liquidity Risk evaluator.
///
/// # Examples
/// ```
/// use rat_risk::liquidity::liquidity::LiquidityRisk;
///
/// // Position: 500 shares, avg volume 100k, order book depth 50k
/// let risk = LiquidityRisk::new(500.0, 100_000.0, 50_000.0, 5);
/// assert!(risk.calculate() < 0.01); // Very liquid
/// ```
pub struct LiquidityRisk {
    /// Size of the position (in shares/units).
    position_size: f64,
    /// Average daily volume (or average volume per bar).
    avg_volume: f64,
    /// Visible order book depth on one side (in shares/units).
    order_book_depth: f64,
    /// Number of bars/windows available to liquidate.
    bars_to_liquidate: u32,
}

impl LiquidityRisk {
    pub fn name() -> &'static str {
        "LiquidityRisk"
    }

    /// Create a new LiquidityRisk evaluator.
    ///
    /// # Arguments
    /// * `position_size` — Number of shares/units held.
    /// * `avg_volume` — Average volume per bar (use the bar timeframe you care about).
    /// * `order_book_depth` — Visible depth on the sell side (shares/units at nearby prices).
    /// * `bars_to_liquidate` — How many bars you have to exit.
    pub fn new(
        position_size: f64,
        avg_volume: f64,
        order_book_depth: f64,
        bars_to_liquidate: u32,
    ) -> Self {
        Self {
            position_size,
            avg_volume,
            order_book_depth,
            bars_to_liquidate,
        }
    }

    /// Calculate liquidity risk score.
    ///
    /// Returns 0.0 (very liquid) to 1.0 (illiquid / high slippage risk).
    ///
    /// Two components:
    /// 1. **Volume participation**: What fraction of avg volume does the position represent?
    ///    > 10% per bar = high risk.
    /// 2. **Depth coverage**: Can the order book absorb the position at nearby prices?
    ///    Position > order book depth = slippage risk.
    ///
    /// The final score is the max of the two risk components, clamped to [0, 1].
    pub fn calculate(&self) -> f64 {
        if self.position_size <= 0.0 {
            return 0.0;
        }

        // Risk 1: Volume participation rate
        // What fraction of volume we need to consume per bar
        let bars = self.bars_to_liquidate.max(1) as f64;
        let volume_per_bar = if self.avg_volume > 0.0 {
            self.avg_volume
        } else {
            return 1.0; // No volume = maximum risk
        };
        let participation_rate = self.position_size / (volume_per_bar * bars);

        // Risk 2: Order book depth coverage
        // If position > depth, we'll walk the book and get slippage
        let depth_ratio = if self.order_book_depth > 0.0 {
            self.position_size / self.order_book_depth
        } else {
            // No depth data: assume high risk if position is large
            if self.position_size > 0.0 {
                2.0
            } else {
                0.0
            }
        };

        // Map to 0-1 scale
        // participation_rate: 0.1 (10%) → 0.5 risk, 1.0 (100%) → 1.0 risk
        let vol_risk = (participation_rate * 2.0).min(1.0);
        // depth_ratio: 1.0 (position = depth) → 0.5 risk, 2.0+ → 1.0 risk
        let depth_risk = (depth_ratio * 0.5).min(1.0);

        // Take the worse of the two
        vol_risk.max(depth_risk)
    }

    /// Convenience: estimated slippage in percentage terms.
    ///
    /// Rough model: slippage ≈ (position / depth) * 0.5% when position > depth.
    pub fn estimated_slippage_pct(&self) -> f64 {
        if self.order_book_depth <= 0.0 || self.position_size <= 0.0 {
            return 0.0;
        }
        let ratio = self.position_size / self.order_book_depth;
        if ratio <= 1.0 {
            0.0
        } else {
            // Linear slippage model: each unit beyond depth costs ~0.5% more
            ((ratio - 1.0) * 0.5).min(10.0)
        }
    }

    /// Convenience: can the position be liquidated within N bars at <1% slippage?
    pub fn can_liquidate_safely(&self) -> bool {
        self.estimated_slippage_pct() < 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_very_liquid() {
        // Tiny position relative to volume and depth
        let risk = LiquidityRisk::new(100.0, 1_000_000.0, 500_000.0, 5);
        assert!(risk.calculate() < 0.01, "Expected negligible risk, got {}", risk.calculate());
        assert!(risk.can_liquidate_safely());
    }

    #[test]
    fn test_illiquid() {
        // Position equals order book depth — will walk the book
        let risk = LiquidityRisk::new(100_000.0, 200_000.0, 100_000.0, 1);
        assert!(risk.calculate() > 0.0);
    }

    #[test]
    fn test_no_volume() {
        let risk = LiquidityRisk::new(1000.0, 0.0, 5000.0, 5);
        assert_eq!(risk.calculate(), 1.0); // Maximum risk
    }

    #[test]
    fn test_zero_position() {
        let risk = LiquidityRisk::new(0.0, 100_000.0, 50_000.0, 5);
        assert_eq!(risk.calculate(), 0.0);
    }

    #[test]
    fn test_slippage_estimate() {
        // Position = 2x depth → slippage ≈ 0.5%
        let risk = LiquidityRisk::new(100_000.0, 1_000_000.0, 50_000.0, 5);
        let slippage = risk.estimated_slippage_pct();
        assert!((slippage - 0.5).abs() < 0.01, "Expected ~0.5%, got {}", slippage);
        assert!(risk.can_liquidate_safely()); // 0.5% < 1%
    }

    #[test]
    fn test_high_slippage() {
        // Position = 5x depth → slippage ≈ 2%
        let risk = LiquidityRisk::new(250_000.0, 1_000_000.0, 50_000.0, 1);
        let slippage = risk.estimated_slippage_pct();
        assert!(slippage > 1.0, "Expected >1%, got {}", slippage);
        assert!(!risk.can_liquidate_safely());
    }

    #[test]
    fn test_multi_bar_liquidation() {
        // Large position but spread over 20 bars reduces per-bar pressure
        let risk = LiquidityRisk::new(500_000.0, 100_000.0, 500_000.0, 20);
        // 500k / (100k * 20) = 0.25 participation, depth covers it → moderate risk
        assert!(risk.calculate() > 0.0);
        assert!(risk.calculate() < 1.0);
    }
}
