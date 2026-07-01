//! RSI (Relative Strength Index) Indicator
//!
//! Measures momentum by comparing average gains to average losses.
//! Values: 0-100, where >70 = overbought, <30 = oversold.

pub struct RsiIndicator {
    pub period: usize,
    pub overbought: f64,
    pub oversold: f64,
}

impl RsiIndicator {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            overbought: 70.0,
            oversold: 30.0,
        }
    }

    pub fn calculate(&self, prices: &[f64]) -> Vec<f64> {
        if prices.len() < self.period + 1 {
            return vec![];
        }

        let mut gains = Vec::new();
        let mut losses = Vec::new();

        for i in 1..prices.len() {
            let change = prices[i] - prices[i - 1];
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push(-change);
            }
        }

        let mut rsi_values = Vec::new();
        let mut avg_gain = gains[..self.period].iter().sum::<f64>() / self.period as f64;
        let mut avg_loss = losses[..self.period].iter().sum::<f64>() / self.period as f64;

        for i in self.period..gains.len() {
            avg_gain = (avg_gain * (self.period - 1) as f64 + gains[i]) / self.period as f64;
            avg_loss = (avg_loss * (self.period - 1) as f64 + losses[i]) / self.period as f64;

            let rs = if avg_loss > 0.0 { avg_gain / avg_loss } else { 100.0 };
            let rsi = 100.0 - (100.0 / (1.0 + rs));
            rsi_values.push(rsi);
        }

        rsi_values
    }

    pub fn signal(&self, rsi: f64) -> RsiSignal {
        if rsi >= self.overbought {
            RsiSignal::Overbought
        } else if rsi <= self.oversold {
            RsiSignal::Oversold
        } else {
            RsiSignal::Neutral
        }
    }
}

pub enum RsiSignal {
    Overbought,
    Oversold,
    Neutral,
}
