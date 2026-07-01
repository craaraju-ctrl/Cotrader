//! Arbitrage Strategy
//! Compare price across two exchanges.
//! If spread > fees + slippage, execute both legs.

use crate::Signal;

pub struct ArbitrageStrategy;

impl ArbitrageStrategy {
    pub fn name() -> &'static str { "ArbitrageStrategy" }

    /// prices_exchange_a and prices_exchange_b are the same asset on two exchanges.
    /// fee_rate is the combined round-trip fee (e.g., 0.002 for 0.1% each way).
    /// slippage is the estimated slippage per leg (e.g., 0.001).
    pub fn generate_signal(
        &self,
        prices_exchange_a: &[f64],
        prices_exchange_b: &[f64],
        fee_rate: f64,
        slippage: f64,
    ) -> Signal {
        if prices_exchange_a.is_empty() || prices_exchange_b.is_empty() {
            return Signal::hold();
        }

        let price_a = *prices_exchange_a.last().unwrap();
        let price_b = *prices_exchange_b.last().unwrap();
        let total_cost = fee_rate + slippage * 2.0;

        if price_a == 0.0 || price_b == 0.0 {
            return Signal::hold();
        }

        let spread_abs = (price_a - price_b).abs();
        let spread_pct = spread_abs / price_a.min(price_b);

        if spread_pct > total_cost {
            // Arbitrage opportunity exists
            let profit_pct = spread_pct - total_cost;
            if price_a < price_b {
                // Buy on exchange A, sell on exchange B
                // From the asset's perspective: BUY on A
                let confidence = (profit_pct / total_cost * 0.3 + 0.5).min(0.9);
                return Signal::buy(confidence);
            } else {
                // Buy on exchange B, sell on exchange A
                // From the asset's perspective: SELL on A (we'd be selling A)
                let confidence = (profit_pct / total_cost * 0.3 + 0.5).min(0.9);
                return Signal::sell(confidence);
            }
        }

        Signal::hold()
    }
}
