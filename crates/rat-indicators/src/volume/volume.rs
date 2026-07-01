//! Volume Profile Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the Point of Control (POC) — the price level with the highest volume.

pub struct VolumeIndicator {
    pub num_bins: usize,
}

impl VolumeIndicator {
    pub fn new(num_bins: usize) -> Self {
        Self { num_bins }
    }

    pub fn name() -> &'static str {
        "VolumeIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;
        let num_bins = self.num_bins.max(1);

        let mut min_price = f64::INFINITY;
        let mut max_price = f64::NEG_INFINITY;
        for i in 0..bars {
            let h = data[i * 5 + 1];
            let l = data[i * 5 + 2];
            if h > max_price {
                max_price = h;
            }
            if l < min_price {
                min_price = l;
            }
        }

        let range = (max_price - min_price).max(0.0001);
        let bin_size = range / num_bins as f64;
        let mut bins: Vec<f64> = vec![0.0; num_bins];

        for i in 0..bars {
            let h = data[i * 5 + 1];
            let l = data[i * 5 + 2];
            let c = data[i * 5 + 3];
            let v = data[i * 5 + 4];
            let tp = (h + l + c) / 3.0;
            let bin_idx = ((tp - min_price) / bin_size).min(num_bins as f64 - 1.0) as usize;
            bins[bin_idx] += v;
        }

        // Find POC (highest volume bin)
        let mut poc_idx = 0;
        let mut max_vol = 0.0;
        for (i, &vol) in bins.iter().enumerate() {
            if vol > max_vol {
                max_vol = vol;
                poc_idx = i;
            }
        }

        // Return POC price
        min_price + (poc_idx as f64 + 0.5) * bin_size
    }
}
