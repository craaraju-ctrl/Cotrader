//! CCI (Commodity Channel Index) Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the current CCI value. >100 = overbought, <-100 = oversold.

pub struct CciIndicator {
    pub period: usize,
}

impl CciIndicator {
    pub fn new(period: usize) -> Self {
        Self { period }
    }

    pub fn name() -> &'static str {
        "CciIndicator"
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

        // Compute typical prices
        let tp: Vec<f64> = (0..bars)
            .map(|i| (data[i * 5 + 1] + data[i * 5 + 2] + data[i * 5 + 3]) / 3.0)
            .collect();

        // SMA of typical price over period
        let sma: f64 = tp[(bars - period)..bars].iter().sum::<f64>() / period as f64;

        // Mean deviation
        let mean_dev: f64 = tp[(bars - period)..bars]
            .iter()
            .map(|&x| (x - sma).abs())
            .sum::<f64>()
            / period as f64;

        if mean_dev == 0.0 {
            return 0.0;
        }

        // CCI = (TP - SMA(TP, period)) / (0.015 * MeanDeviation)
        (tp[bars - 1] - sma) / (0.015 * mean_dev)
    }
}
