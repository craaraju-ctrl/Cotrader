//! Pairs Strategy
//! Monitor correlation between two assets.
//! When spread > 2 std dev, go long cheap / short expensive.

use crate::Signal;

pub struct PairsStrategy;

impl PairsStrategy {
    pub fn name() -> &'static str { "PairsStrategy" }

    /// prices_a and prices_b are aligned closing prices for the two assets.
    /// The most recent entry is the current price.
    pub fn generate_signal(&self, prices_a: &[f64], prices_b: &[f64]) -> Signal {
        let len = prices_a.len().min(prices_b.len());
        if len < 20 {
            return Signal::hold();
        }
        let a = &prices_a[prices_a.len() - len..];
        let b = &prices_b[prices_b.len() - len..];

        // Calculate spread as ratio
        let mut spreads: Vec<f64> = Vec::with_capacity(len);
        for i in 0..len {
            if b[i] != 0.0 {
                spreads.push(a[i] / b[i]);
            }
        }
        if spreads.len() < 20 {
            return Signal::hold();
        }

        let mean: f64 = spreads.iter().sum::<f64>() / spreads.len() as f64;
        let variance: f64 = spreads.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / spreads.len() as f64;
        let std_dev = variance.sqrt();
        if std_dev == 0.0 {
            return Signal::hold();
        }

        let current_spread = *spreads.last().unwrap();
        let z_score = (current_spread - mean) / std_dev;

        if z_score > 2.0 {
            // Asset A is expensive relative to B → sell A, buy B
            // From the perspective of asset A: SELL signal
            let confidence = (z_score - 2.0) / 3.0 * 0.4 + 0.5;
            return Signal::sell(confidence.min(0.9));
        }

        if z_score < -2.0 {
            // Asset A is cheap relative to B → buy A, sell B
            // From the perspective of asset A: BUY signal
            let confidence = (2.0 + z_score).abs() / 3.0 * 0.4 + 0.5;
            return Signal::buy(confidence.min(0.9));
        }

        // Mean reversion exit: spread back near 0
        if z_score.abs() < 0.5 {
            // Close to mean — could close pairs position
            return Signal::hold();
        }

        Signal::hold()
    }
}
