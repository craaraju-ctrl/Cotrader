//! Williams %R Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns %R value (-100 to 0). <-80 = oversold, >-20 = overbought.

pub struct WilliamsIndicator {
    pub period: usize,
}

impl WilliamsIndicator {
    pub fn new(period: usize) -> Self {
        Self { period }
    }

    pub fn name() -> &'static str {
        "WilliamsIndicator"
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

        // Find highest high and lowest low over the period
        let start = bars - period;
        let mut highest = f64::NEG_INFINITY;
        let mut lowest = f64::INFINITY;
        for i in start..bars {
            let h = data[i * 5 + 1];
            let l = data[i * 5 + 2];
            if h > highest {
                highest = h;
            }
            if l < lowest {
                lowest = l;
            }
        }

        let close = data[(bars - 1) * 5 + 3];
        let range = highest - lowest;

        if range == 0.0 {
            return -50.0;
        }

        // %R = (Highest High - Close) / (Highest High - Lowest Low) * -100
        ((highest - close) / range) * -100.0
    }
}
