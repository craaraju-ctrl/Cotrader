//! Donchian Channel Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the middle band (upper + lower) / 2.

pub struct DonchianIndicator {
    pub period: usize,
}

impl DonchianIndicator {
    pub fn new(period: usize) -> Self {
        Self { period }
    }

    pub fn name() -> &'static str {
        "DonchianIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;
        let period = self.period;

        if bars < period {
            return 0.0;
        }

        let start = bars - period;
        let highs = &data[start * 5 + 1..(start * 5 + 1) + period * 5]
            .iter()
            .step_by(5)
            .copied()
            .collect::<Vec<f64>>();
        let lows = &data[start * 5 + 2..(start * 5 + 2) + period * 5]
            .iter()
            .step_by(5)
            .copied()
            .collect::<Vec<f64>>();

        let upper = highs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let lower = lows.iter().cloned().fold(f64::INFINITY, f64::min);

        (upper + lower) / 2.0
    }
}
