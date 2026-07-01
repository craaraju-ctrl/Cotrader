//! Momentum Strategy
//! Enter when 14-period ROC > 0 (BUY) and volume > average (confirmation).
//! Exit when ROC < 0.

use crate::Signal;

pub struct MomentumStrategy;

impl MomentumStrategy {
    pub fn name() -> &'static str { "MomentumStrategy" }

    pub fn generate_signal(&self, closes: &[f64], volumes: &[f64]) -> Signal {
        if closes.len() < 15 || volumes.len() < 15 {
            return Signal::hold();
        }

        let roc_period = 14;
        let price_now = closes[closes.len() - 1];
        let price_ago = closes[closes.len() - 1 - roc_period];
        let roc = (price_now - price_ago) / price_ago;

        let avg_vol: f64 = volumes[volumes.len() - 14..].iter().sum::<f64>() / 14.0;
        let current_vol = volumes[volumes.len() - 1];
        let vol_ratio = current_vol / avg_vol;

        if roc > 0.0 && vol_ratio > 1.0 {
            // Positive ROC with above-average volume confirmation
            let confidence = (roc * 10.0 + (vol_ratio - 1.0) * 0.3).min(0.9);
            return Signal::buy(confidence.max(0.5));
        }

        if roc < 0.0 {
            let confidence = (roc.abs() * 10.0).min(0.9);
            return Signal::sell(confidence.max(0.5));
        }

        Signal::hold()
    }
}
