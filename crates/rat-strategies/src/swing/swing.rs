//! Swing Strategy
//! Multi-day hold. Enter on RSI + MACD alignment.
//! Exit after 3-5 days or when indicators reverse.

use crate::Signal;

pub struct SwingStrategy;

impl SwingStrategy {
    pub fn name() -> &'static str { "SwingStrategy" }

    pub fn generate_signal(&self, closes: &[f64]) -> Signal {
        if closes.len() < 35 {
            return Signal::hold();
        }

        let rsi_now = rsi(closes, 14);
        let macd_line = ema(closes, 12) - ema(closes, 26);
        let prev_closes = &closes[..closes.len() - 1];
        let prev_macd = ema(prev_closes, 12) - ema(prev_closes, 26);

        let signal_val = ema_from_series(&macd_series(closes), 9);
        let prev_signal = ema_from_series(&macd_series(prev_closes), 9);

        let price = closes[closes.len() - 1];
        let sma20 = sma(closes, 20);

        // BUY: RSI < 40 (oversold area) + MACD bullish crossover
        if rsi_now < 40.0 && prev_macd < prev_signal && macd_line > signal_val {
            let confidence = 0.7;
            return Signal::buy(confidence);
        }

        // SELL: RSI > 60 (overbought area) + MACD bearish crossover
        if rsi_now > 60.0 && prev_macd > prev_signal && macd_line < signal_val {
            let confidence = 0.7;
            return Signal::sell(confidence);
        }

        // Secondary entry: RSI crossing up from 30 + price above SMA20
        if rsi_now > 30.0 && rsi(&closes[..closes.len() - 1], 14) < 30.0 && price > sma20 {
            return Signal::buy(0.6);
        }

        // Secondary exit: RSI crossing down from 70
        if rsi_now < 70.0 && rsi(&closes[..closes.len() - 1], 14) > 70.0 {
            return Signal::sell(0.6);
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
        if change > 0.0 { gains += change; } else { losses -= change; }
    }
    if losses == 0.0 { return 100.0; }
    let rs = (gains / period as f64) / (losses / period as f64);
    100.0 - 100.0 / (1.0 + rs)
}

fn ema(data: &[f64], period: usize) -> f64 {
    if data.is_empty() { return 0.0; }
    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema_val = data[0];
    for &price in &data[1..] {
        ema_val = (price - ema_val) * multiplier + ema_val;
    }
    ema_val
}

fn macd_series(closes: &[f64]) -> Vec<f64> {
    let mut series = Vec::with_capacity(closes.len());
    for i in 0..closes.len() {
        let slice = &closes[..=i];
        series.push(ema(slice, 12) - ema(slice, 26));
    }
    series
}

fn ema_from_series(series: &[f64], period: usize) -> f64 {
    if series.is_empty() { return 0.0; }
    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema_val = series[0];
    for &val in &series[1..] {
        ema_val = (val - ema_val) * multiplier + ema_val;
    }
    ema_val
}

fn sma(data: &[f64], period: usize) -> f64 {
    let slice = &data[data.len() - period..];
    slice.iter().sum::<f64>() / period as f64
}
