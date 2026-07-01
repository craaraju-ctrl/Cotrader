//! Money Manager — Controls position sizing and risk allocation.
//!
//! Implements Kelly Criterion, portfolio heat management, and dynamic sizing.

pub struct MoneyManager;

impl MoneyManager {
    pub fn name() -> &'static str { "MoneyManager" }
    pub fn role() -> &'static str { "Money Manager" }

    /// Calculate optimal position size using Kelly Criterion.
    pub fn kelly_size(&self, win_rate: f64, avg_win: f64, avg_loss: f64) -> String {
        todo!("Kelly fraction with half-Kelly safety and volatility adjustment")
    }

    /// Adjust position size based on portfolio heat.
    pub fn heat_adjust(&self, size: f64, heat: f64) -> String {
        todo!("Reduce size when portfolio heat exceeds threshold")
    }

    /// Scale position based on conviction and regime.
    pub fn conviction_scale(&self, base_size: f64, conviction: f64, _regime: &str) -> String {
        todo!("Scale up in high-conviction trending, down in uncertain regimes")
    }

    /// Calculate maximum allowable position.
    pub fn max_position(&self, equity: f64, risk_pct: f64, _stop_distance: f64) -> String {
        todo!("Risk-based maximum position calculation")
    }
}
