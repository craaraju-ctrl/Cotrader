//! VWAP (Volume Weighted Average Price) Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the cumulative VWAP value.

pub struct VwapIndicator;

impl VwapIndicator {
    pub fn name() -> &'static str {
        "VwapIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;
        let mut cum_tp_vol = 0.0;
        let mut cum_vol = 0.0;

        for i in 0..bars {
            let high = data[i * 5 + 1];
            let low = data[i * 5 + 2];
            let close = data[i * 5 + 3];
            let volume = data[i * 5 + 4];
            let tp = (high + low + close) / 3.0;

            cum_tp_vol += tp * volume;
            cum_vol += volume;
        }

        if cum_vol == 0.0 {
            return 0.0;
        }

        cum_tp_vol / cum_vol
    }
}
