//! Breakout Strategy
//! Enter when price breaks above 20-period high (BUY) or below 20-period low (SELL).
//! Use ATR for stop.

use crate::Signal;

pub struct BreakoutStrategy;

impl BreakoutStrategy {
    pub fn name() -> &'static str { "BreakoutStrategy" }

    pub fn generate_signal(&self, closes: &[f64], highs: &[f64], lows: &[f64]) -> Signal {
        if closes.len() < 21 || highs.len() < 21 || lows.len() < 21 {
            return Signal::hold();
        }

        let lookback = 20;
        let price = closes[closes.len() - 1];

        // Calculate 20-period high and low (excluding current bar)
        let recent_highs = &highs[highs.len() - 1 - lookback..highs.len() - 1];
        let recent_lows = &lows[lows.len() - 1 - lookback..lows.len() - 1];
        let period_high = recent_highs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let period_low = recent_lows.iter().cloned().fold(f64::INFINITY, f64::min);

        let atr = atr(highs, lows, closes, 14);

        if price > period_high {
            // Breakout above 20-period high
            let breakout_strength = (price - period_high) / atr;
            let confidence = (0.5 + breakout_strength * 0.15).min(0.9);
            return Signal::buy(confidence.max(0.5));
        }

        if price < period_low {
            // Breakdown below 20-period low
            let breakdown_strength = (period_low - price) / atr;
            let confidence = (0.5 + breakdown_strength * 0.15).min(0.9);
            return Signal::sell(confidence.max(0.5));
        }

        Signal::hold()
    }
}

fn atr(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> f64 {
    if highs.len() < period + 1 {
        return 1.0;
    }
    let mut tr_sum = 0.0;
    for i in (highs.len() - period)..highs.len() {
        let tr = (highs[i] - lows[i])
            .max((highs[i] - closes[i - 1]).abs())
            .max((lows[i] - closes[i - 1]).abs());
        tr_sum += tr;
    }
    tr_sum / period as f64
}
