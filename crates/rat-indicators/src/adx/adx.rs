//! ADX (Average Directional Index) Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the current ADX value (0–100). >25 = trending, <20 = ranging.

pub struct AdxIndicator {
    pub period: usize,
}

impl AdxIndicator {
    pub fn new(period: usize) -> Self {
        Self { period }
    }

    pub fn name() -> &'static str {
        "AdxIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;
        let period = self.period;

        if bars < period + 1 {
            return 0.0;
        }

        // Extract high, low, close arrays
        let highs: Vec<f64> = (0..bars).map(|i| data[i * 5 + 1]).collect();
        let lows: Vec<f64> = (0..bars).map(|i| data[i * 5 + 2]).collect();
        let closes: Vec<f64> = (0..bars).map(|i| data[i * 5 + 3]).collect();

        // True Range for first bar
        let mut tr = vec![0.0; bars];
        tr[0] = highs[0] - lows[0];
        for i in 1..bars {
            tr[i] = (highs[i] - lows[i])
                .max((highs[i] - closes[i - 1]).abs())
                .max((lows[i] - closes[i - 1]).abs());
        }

        // Directional Movement
        let mut plus_dm = vec![0.0; bars];
        let mut minus_dm = vec![0.0; bars];
        for i in 1..bars {
            let up = highs[i] - highs[i - 1];
            let down = lows[i - 1] - lows[i];
            if up > down && up > 0.0 {
                plus_dm[i] = up;
            }
            if down > up && down > 0.0 {
                minus_dm[i] = down;
            }
        }

        // Wilder smoothing (EMA with period factor)
        let alpha = 1.0 / period as f64;

        let mut smoothed_tr = 0.0;
        let mut smoothed_plus = 0.0;
        let mut smoothed_minus = 0.0;

        for i in 1..=period {
            smoothed_tr += tr[i];
            smoothed_plus += plus_dm[i];
            smoothed_minus += minus_dm[i];
        }

        let mut atr_vals = vec![0.0; bars];
        let mut plus_di_vals = vec![0.0; bars];
        let mut minus_di_vals = vec![0.0; bars];

        atr_vals[period] = smoothed_tr;
        plus_di_vals[period] = smoothed_plus;
        minus_di_vals[period] = smoothed_minus;

        for i in (period + 1)..bars {
            smoothed_tr = smoothed_tr - smoothed_tr * alpha + tr[i];
            smoothed_plus = smoothed_plus - smoothed_plus * alpha + plus_dm[i];
            smoothed_minus = smoothed_minus - smoothed_minus * alpha + minus_dm[i];
            atr_vals[i] = smoothed_tr;
            plus_di_vals[i] = smoothed_plus;
            minus_di_vals[i] = smoothed_minus;
        }

        // Compute DI+ and DI-
        let mut dx_vals = Vec::new();
        for i in period..bars {
            if atr_vals[i] > 0.0 {
                let plus_di = 100.0 * plus_di_vals[i] / atr_vals[i];
                let minus_di = 100.0 * minus_di_vals[i] / atr_vals[i];
                let di_sum = plus_di + minus_di;
                if di_sum > 0.0 {
                    dx_vals.push(100.0 * (plus_di - minus_di).abs() / di_sum);
                } else {
                    dx_vals.push(0.0);
                }
            } else {
                dx_vals.push(0.0);
            }
        }

        if dx_vals.len() < period {
            return 0.0;
        }

        // First ADX = SMA of first `period` DX values
        let mut adx = dx_vals[..period].iter().sum::<f64>() / period as f64;

        // Subsequent ADX = smoothed
        for &dx in &dx_vals[period..] {
            adx = (adx * (period as f64 - 1.0) + dx) / period as f64;
        }

        adx
    }
}
