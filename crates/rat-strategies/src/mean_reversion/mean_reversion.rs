//! MeanReversion Strategy
//! Enter when RSI < 30 (BUY) or RSI > 70 (SELL).
//! Exit when RSI crosses 50.

use crate::Signal;

pub struct MeanReversionStrategy;

impl MeanReversionStrategy {
    pub fn name() -> &'static str { "MeanReversionStrategy" }

    pub fn generate_signal(&self, closes: &[f64]) -> Signal {
        if closes.len() < 15 {
            return Signal::hold();
        }

        let rsi_now = rsi(closes, 14);
        let rsi_prev = rsi(&closes[..closes.len() - 1], 14);

        if rsi_now < 30.0 {
            // Oversold — buy signal
            let confidence = (30.0 - rsi_now) / 30.0 * 0.5 + 0.4;
            return Signal::buy(confidence.min(0.9));
        }

        if rsi_now > 70.0 {
            // Overbought — sell signal
            let confidence = (rsi_now - 70.0) / 30.0 * 0.5 + 0.4;
            return Signal::sell(confidence.min(0.9));
        }

        // Exit signal: RSI crosses 50
        if (rsi_prev < 50.0 && rsi_now > 50.0) || (rsi_prev > 50.0 && rsi_now < 50.0) {
            let confidence = 0.5;
            if rsi_now > 50.0 {
                return Signal::sell(confidence);
            } else {
                return Signal::buy(confidence);
            }
        }

        Signal::hold()
    }
}

fn rsi(closes: &[f64], period: usize) -> f64 {
    if closes.len() < period + 1 {
        return 50.0;
    }
    let mut gains = 0.0;
    let mut losses = 0.0;
    for i in (closes.len() - period)..closes.len() {
        let change = closes[i] - closes[i - 1];
        if change > 0.0 {
            gains += change;
        } else {
            losses -= change;
        }
    }
    if losses == 0.0 {
        return 100.0;
    }
    let rs = (gains / period as f64) / (losses / period as f64);
    100.0 - 100.0 / (1.0 + rs)
}
