//! Keltner Channel Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the middle line (EMA of close, period 20).

pub struct KeltnerIndicator;

impl KeltnerIndicator {
    pub fn name() -> &'static str {
        "KeltnerIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;

        if bars < 20 {
            return 0.0;
        }

        let closes: Vec<f64> = (0..bars).map(|i| data[i * 5 + 3]).collect();
        let highs: Vec<f64> = (0..bars).map(|i| data[i * 5 + 1]).collect();
        let lows: Vec<f64> = (0..bars).map(|i| data[i * 5 + 2]).collect();

        // EMA of close (period 20)
        let multiplier = 2.0 / 21.0;
        let mut ema = closes[..20].iter().sum::<f64>() / 20.0;
        for &c in &closes[20..] {
            ema = c * multiplier + ema * (1.0 - multiplier);
        }

        // ATR(10) for channel width
        let atr_period = 10;
        let mut tr_sum = 0.0;
        tr_sum += highs[0] - lows[0];
        for i in 1..bars {
            let tr = (highs[i] - lows[i])
                .max((highs[i] - closes[i - 1]).abs())
                .max((lows[i] - closes[i - 1]).abs());
            if i < atr_period {
                tr_sum += tr;
            }
        }
        let mut atr = tr_sum / atr_period as f64;

        // Smooth ATR
        for i in (atr_period + 1)..bars {
            let tr = (highs[i] - lows[i])
                .max((highs[i] - closes[i - 1]).abs())
                .max((lows[i] - closes[i - 1]).abs());
            atr = (atr * (atr_period as f64 - 1.0) + tr) / atr_period as f64;
        }

        // Return middle line (EMA); upper/lower can be computed as EMA ± 2*ATR
        ema
    }
}
