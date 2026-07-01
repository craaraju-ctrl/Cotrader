//! Grid Strategy
//! Place buy/sell orders at fixed intervals above and below current price.

use crate::Signal;

pub struct GridStrategy;

impl GridStrategy {
    pub fn name() -> &'static str { "GridStrategy" }

    /// current_price: the current mid price
    /// grid_size_pct: the grid spacing as a percentage (e.g., 0.005 for 0.5%)
    /// recent_prices: used to detect trend (skip grid in strong trends)
    pub fn generate_signal(&self, current_price: f64, grid_size_pct: f64, recent_prices: &[f64]) -> Signal {
        if current_price <= 0.0 || grid_size_pct <= 0.0 || recent_prices.len() < 20 {
            return Signal::hold();
        }

        // Check if market is trending (grid works best in ranging markets)
        let sma_short = sma(recent_prices, 5);
        let sma_long = sma(recent_prices, 20);
        let trend = (sma_short - sma_long) / sma_long;

        // Don't grid in strong trends
        if trend.abs() > 0.02 {
            return Signal::hold();
        }

        // Calculate which grid level we're at
        // Grid levels are at price * (1 + n * grid_size_pct) for n = ..., -2, -1, 0, 1, 2, ...
        let grid_ratio = (current_price / recent_prices[0]).ln() / (1.0 + grid_size_pct).ln();
        let fractional = grid_ratio - grid_ratio.floor();

        // Buy at lower grid levels (fractional < 0.3), sell at upper levels (fractional > 0.7)
        if fractional < 0.3 {
            // Near a buy grid level
            let proximity = 1.0 - fractional / 0.3;
            let confidence = 0.4 + proximity * 0.3;
            return Signal::buy(confidence.min(0.7));
        }

        if fractional > 0.7 {
            // Near a sell grid level
            let proximity = (fractional - 0.7) / 0.3;
            let confidence = 0.4 + proximity * 0.3;
            return Signal::sell(confidence.min(0.7));
        }

        Signal::hold()
    }
}

fn sma(data: &[f64], period: usize) -> f64 {
    let len = data.len().min(period);
    if len == 0 { return 0.0; }
    let slice = &data[data.len() - len..];
    slice.iter().sum::<f64>() / len as f64
}
