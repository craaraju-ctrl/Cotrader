//! MACD (Moving Average Convergence Divergence) Indicator
//!
//! Trend-following momentum indicator showing relationship between two EMAs.

pub struct MacdIndicator {
    pub fast_period: usize,
    pub slow_period: usize,
    pub signal_period: usize,
}

impl MacdIndicator {
    pub fn new(fast: usize, slow: usize, signal: usize) -> Self {
        Self {
            fast_period: fast,
            slow_period: slow,
            signal_period: signal,
        }
    }

    pub fn calculate(&self, prices: &[f64]) -> MacdResult {
        let fast_ema = self.ema(prices, self.fast_period);
        let slow_ema = self.ema(prices, self.slow_period);

        let mut macd_line = Vec::new();
        let offset = self.slow_period - self.fast_period;
        for i in offset..fast_ema.len() {
            macd_line.push(fast_ema[i] - slow_ema[i - offset]);
        }

        let signal_line = self.ema(&macd_line, self.signal_period);

        let mut histogram = Vec::new();
        let signal_offset = self.slow_period - 1;
        for i in 0..signal_line.len() {
            if i + signal_offset < macd_line.len() {
                histogram.push(macd_line[i + signal_offset] - signal_line[i]);
            }
        }

        MacdResult {
            macd_line,
            signal_line,
            histogram,
        }
    }

    fn ema(&self, prices: &[f64], period: usize) -> Vec<f64> {
        if prices.is_empty() || period == 0 {
            return vec![];
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(prices.len());

        // Start with SMA for first period
        let sma: f64 = prices[..period].iter().sum::<f64>() / period as f64;
        ema.push(sma);

        for i in period..prices.len() {
            let value = prices[i] * multiplier + ema.last().unwrap() * (1.0 - multiplier);
            ema.push(value);
        }

        ema
    }

    pub fn signal(&self, macd: f64, signal: f64, prev_macd: f64, prev_signal: f64) -> MacdSignal {
        let crossover = macd > signal && prev_macd <= prev_signal;
        let crossunder = macd < signal && prev_macd >= prev_signal;

        if crossover {
            MacdSignal::BullishCrossover
        } else if crossunder {
            MacdSignal::BearishCrossover
        } else if macd > signal {
            MacdSignal::Bullish
        } else {
            MacdSignal::Bearish
        }
    }
}

pub struct MacdResult {
    pub macd_line: Vec<f64>,
    pub signal_line: Vec<f64>,
    pub histogram: Vec<f64>,
}

pub enum MacdSignal {
    BullishCrossover,
    BearishCrossover,
    Bullish,
    Bearish,
}
