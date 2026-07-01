//! TrendFollowing Strategy
//! Enter when price crosses above 20-SMA (BUY) or below (SELL).
//! Use 50-SMA as trend filter.

use crate::Signal;

pub struct TrendFollowingStrategy;

impl TrendFollowingStrategy {
    pub fn name() -> &'static str { "TrendFollowingStrategy" }

    pub fn generate_signal(&self, closes: &[f64]) -> Signal {
        if closes.len() < 51 {
            return Signal::hold();
        }

        let sma20_now = sma(closes, 20);
        let sma20_prev = sma(&closes[..closes.len() - 1], 20);
        let sma50 = sma(closes, 50);
        let price = closes[closes.len() - 1];
        let prev_price = closes[closes.len() - 2];

        // Trend filter: only buy above 50-SMA, only sell below 50-SMA
        if prev_price < sma20_prev && price > sma20_now && price > sma50 {
            // Price crossed above 20-SMA while above 50-SMA
            let distance = (price - sma50) / sma50;
            let confidence = (0.5 + distance * 5.0).min(0.9);
            return Signal::buy(confidence);
        }

        if prev_price > sma20_prev && price < sma20_now && price < sma50 {
            // Price crossed below 20-SMA while below 50-SMA
            let distance = (sma50 - price) / sma50;
            let confidence = (0.5 + distance * 5.0).min(0.9);
            return Signal::sell(confidence);
        }

        Signal::hold()
    }
}

fn sma(data: &[f64], period: usize) -> f64 {
    let slice = &data[data.len() - period..];
    slice.iter().sum::<f64>() / period as f64
}
