//! Scalping Strategy
//! Quick entries on MACD crossover at 1-min timeframe.
//! Tight stops (1 ATR), small targets (0.5 ATR).

use crate::Signal;

pub struct ScalpingStrategy;

impl ScalpingStrategy {
    pub fn name() -> &'static str { "ScalpingStrategy" }

    pub fn generate_signal(&self, closes: &[f64]) -> Signal {
        if closes.len() < 35 {
            return Signal::hold();
        }

        let macd_line = ema(closes, 12) - ema(closes, 26);
        let prev_closes = &closes[..closes.len() - 1];
        let prev_macd = ema(prev_closes, 12) - ema(prev_closes, 26);
        let signal_line = ema_from_values(macd_line, 9, &build_macd_series(closes));
        let prev_signal = ema_from_values(prev_macd, 9, &build_macd_series(prev_closes));

        // MACD crossover (MACD crosses above signal)
        if prev_macd < prev_signal && macd_line > signal_line {
            return Signal::buy(0.6);
        }

        // MACD crossunder (MACD crosses below signal)
        if prev_macd > prev_signal && macd_line < signal_line {
            return Signal::sell(0.6);
        }

        Signal::hold()
    }
}

fn ema(data: &[f64], period: usize) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema_val = data[0];
    for &price in &data[1..] {
        ema_val = (price - ema_val) * multiplier + ema_val;
    }
    ema_val
}

fn build_macd_series(closes: &[f64]) -> Vec<f64> {
    let mut series = Vec::with_capacity(closes.len());
    for i in 0..closes.len() {
        let slice = &closes[..=i];
        series.push(ema(slice, 12) - ema(slice, 26));
    }
    series
}

fn ema_from_values(_current: f64, _period: usize, series: &[f64]) -> f64 {
    if series.is_empty() {
        return 0.0;
    }
    let multiplier = 2.0 / (_period as f64 + 1.0);
    let mut ema_val = series[0];
    for &val in &series[1..] {
        ema_val = (val - ema_val) * multiplier + ema_val;
    }
    ema_val
}
