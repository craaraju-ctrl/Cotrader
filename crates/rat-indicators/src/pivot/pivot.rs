//! Pivot Points Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the Pivot Point (PP = (H + L + C) / 3) of the most recent bar.

pub struct PivotIndicator;

impl PivotIndicator {
    pub fn name() -> &'static str {
        "PivotIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;
        let last = bars - 1;

        let high = data[last * 5 + 1];
        let low = data[last * 5 + 2];
        let close = data[last * 5 + 3];

        // PP = (H + L + C) / 3
        (high + low + close) / 3.0
    }
}
