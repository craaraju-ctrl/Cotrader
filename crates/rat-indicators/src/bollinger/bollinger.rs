//! Bollinger Bands Indicator
//!
//! Volatility bands placed above and below a moving average.
//! Upper band = SMA + (stddev * multiplier)
//! Lower band = SMA - (stddev * multiplier)

pub struct BollingerIndicator {
    pub period: usize,
    pub multiplier: f64,
}

impl BollingerIndicator {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self { period, multiplier }
    }

    pub fn calculate(&self, prices: &[f64]) -> BollingerBands {
        if prices.len() < self.period {
            return BollingerBands {
                upper: vec![],
                middle: vec![],
                lower: vec![],
                bandwidth: vec![],
            };
        }

        let mut upper = Vec::new();
        let mut middle = Vec::new();
        let mut lower = Vec::new();
        let mut bandwidth = Vec::new();

        for i in (self.period - 1)..prices.len() {
            let window = &prices[i + 1 - self.period..=i];
            let sma = window.iter().sum::<f64>() / self.period as f64;
            let variance = window.iter().map(|x| (x - sma).powi(2)).sum::<f64>() / self.period as f64;
            let stddev = variance.sqrt();

            let upper_band = sma + (stddev * self.multiplier);
            let lower_band = sma - (stddev * self.multiplier);
            let bw = if sma > 0.0 { (upper_band - lower_band) / sma } else { 0.0 };

            upper.push(upper_band);
            middle.push(sma);
            lower.push(lower_band);
            bandwidth.push(bw);
        }

        BollingerBands {
            upper,
            middle,
            lower,
            bandwidth,
        }
    }

    pub fn signal(&self, price: f64, upper: f64, lower: f64) -> BollingerSignal {
        if price > upper {
            BollingerSignal::AboveUpper
        } else if price < lower {
            BollingerSignal::BelowLower
        } else if price > (upper + lower) / 2.0 {
            BollingerSignal::AboveMiddle
        } else {
            BollingerSignal::BelowMiddle
        }
    }
}

pub struct BollingerBands {
    pub upper: Vec<f64>,
    pub middle: Vec<f64>,
    pub lower: Vec<f64>,
    pub bandwidth: Vec<f64>,
}

pub enum BollingerSignal {
    AboveUpper,
    BelowUpper,
    AboveMiddle,
    BelowMiddle,
    BelowLower,
}
