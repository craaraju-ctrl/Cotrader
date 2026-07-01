//! Simulator — Market simulation for backtesting.

pub struct Simulator {
    pub tick_size: f64,
    pub latency_ms: u64,
}

impl Simulator {
    pub fn new(tick_size: f64) -> Self {
        Self {
            tick_size,
            latency_ms: 100,
        }
    }

    /// Simulate market impact for a given order size.
    pub fn simulate_impact(&self, order_size: f64, current_price: f64, volume: f64) -> f64 {
        let impact = order_size / volume;
        current_price * impact * 0.1
    }

    /// Simulate slippage based on order size and liquidity.
    pub fn simulate_slippage(&self, order_size: f64, spread: f64) -> f64 {
        let slippage = spread * 0.5 + (order_size / 1_000_000.0) * spread;
        slippage.max(0.0001)
    }
}
