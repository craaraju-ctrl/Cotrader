//! Fibonacci Retracement Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the 50% retracement level between the swing high and swing low.

pub struct FibonacciIndicator;

impl FibonacciIndicator {
    pub fn name() -> &'static str {
        "FibonacciIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;

        // Find the highest high and lowest low across all data
        let mut high = f64::NEG_INFINITY;
        let mut low = f64::INFINITY;
        for i in 0..bars {
            let h = data[i * 5 + 1];
            let l = data[i * 5 + 2];
            if h > high {
                high = h;
            }
            if l < low {
                low = l;
            }
        }

        let range = high - low;
        if range <= 0.0 {
            return high;
        }

        // Return the 50% retracement level
        high - range * 0.5
    }
}
