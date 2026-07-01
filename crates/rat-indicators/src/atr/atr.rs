//! ATR (Average True Range) Indicator
//!
//! Measures market volatility by calculating the average of true ranges.

pub struct AtrIndicator {
    pub period: usize,
}

impl AtrIndicator {
    pub fn new(period: usize) -> Self {
        Self { period }
    }

    pub fn calculate(&self, highs: &[f64], lows: &[f64], closes: &[f64]) -> Vec<f64> {
        if highs.len() < 2 || lows.len() < 2 || closes.len() < 2 {
            return vec![];
        }

        let mut true_ranges = Vec::new();
        true_ranges.push(highs[0] - lows[0]); // First bar is just high-low

        for i in 1..highs.len() {
            let tr = (highs[i] - lows[i])
                .max((highs[i] - closes[i - 1]).abs())
                .max((lows[i] - closes[i - 1]).abs());
            true_ranges.push(tr);
        }

        let mut atr = Vec::new();
        let first_atr: f64 = true_ranges[..self.period].iter().sum::<f64>() / self.period as f64;
        atr.push(first_atr);

        for i in self.period..true_ranges.len() {
            let value = (atr.last().unwrap() * (self.period - 1) as f64 + true_ranges[i]) / self.period as f64;
            atr.push(value);
        }

        atr
    }

    pub fn atr_pct(&self, atr: f64, price: f64) -> f64 {
        if price > 0.0 {
            atr / price * 100.0
        } else {
            0.0
        }
    }
}
