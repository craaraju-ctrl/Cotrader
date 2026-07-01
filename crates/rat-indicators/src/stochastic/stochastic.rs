//! Stochastic Oscillator Indicator
//!
//! Compares closing price to price range over a period.
//! %K = (Close - Low) / (High - Low) * 100
//! %D = SMA of %K

pub struct StochasticIndicator {
    pub k_period: usize,
    pub d_period: usize,
    pub smooth_k: usize,
}

impl StochasticIndicator {
    pub fn new(k_period: usize, d_period: usize, smooth_k: usize) -> Self {
        Self { k_period, d_period, smooth_k }
    }

    pub fn calculate(&self, highs: &[f64], lows: &[f64], closes: &[f64]) -> StochasticResult {
        if highs.len() < self.k_period {
            return StochasticResult { k: vec![], d: vec![] };
        }

        let mut k_values = Vec::new();
        for i in (self.k_period - 1)..highs.len() {
            let period_highs = &highs[i + 1 - self.k_period..=i];
            let period_lows = &lows[i + 1 - self.k_period..=i];
            let highest = period_highs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let lowest = period_lows.iter().cloned().fold(f64::INFINITY, f64::min);

            let k = if highest != lowest {
                (closes[i] - lowest) / (highest - lowest) * 100.0
            } else {
                50.0
            };
            k_values.push(k);
        }

        // Smooth %K
        let smoothed_k = if self.smooth_k > 1 {
            self.sma(&k_values, self.smooth_k)
        } else {
            k_values.clone()
        };

        // Calculate %D as SMA of smoothed %K
        let d_values = self.sma(&smoothed_k, self.d_period);

        StochasticResult { k: smoothed_k, d: d_values }
    }

    fn sma(&self, data: &[f64], period: usize) -> Vec<f64> {
        if data.len() < period { return vec![]; }
        let mut result = Vec::new();
        for i in (period - 1)..data.len() {
            let sum: f64 = data[i + 1 - period..=i].iter().sum();
            result.push(sum / period as f64);
        }
        result
    }

    pub fn signal(&self, k: f64, d: f64) -> StochasticSignal {
        if k > 80.0 && d > 80.0 {
            StochasticSignal::Overbought
        } else if k < 20.0 && d < 20.0 {
            StochasticSignal::Oversold
        } else if k > d {
            StochasticSignal::Bullish
        } else {
            StochasticSignal::Bearish
        }
    }
}

pub struct StochasticResult {
    pub k: Vec<f64>,
    pub d: Vec<f64>,
}

pub enum StochasticSignal {
    Overbought,
    Oversold,
    Bullish,
    Bearish,
}
