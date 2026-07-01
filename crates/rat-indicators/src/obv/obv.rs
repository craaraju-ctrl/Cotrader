//! OBV (On-Balance Volume) Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the latest OBV value.

pub struct ObvIndicator;

impl ObvIndicator {
    pub fn name() -> &'static str {
        "ObvIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 10 {
            return 0.0;
        }

        let bars = data.len() / 5;
        let mut obv = 0.0;

        let mut prev_close = data[3]; // first bar's close
        for i in 1..bars {
            let close = data[i * 5 + 3];
            let volume = data[i * 5 + 4];

            if close > prev_close {
                obv += volume;
            } else if close < prev_close {
                obv -= volume;
            }
            // If close == prev_close, OBV unchanged

            prev_close = close;
        }

        obv
    }
}
