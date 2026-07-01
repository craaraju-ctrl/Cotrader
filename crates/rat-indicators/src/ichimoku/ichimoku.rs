//! Ichimoku Cloud Indicator
//!
//! Data layout: packed OHLCV — [open, high, low, close, volume] per bar.
//! Returns the Senkou Span A (primary cloud line).

pub struct IchimokuIndicator;

impl IchimokuIndicator {
    pub fn name() -> &'static str {
        "IchimokuIndicator"
    }

    pub fn calculate(&self, data: &[f64]) -> f64 {
        if data.len() < 5 {
            return 0.0;
        }

        let bars = data.len() / 5;

        // Need at least 52 bars for Senkou B
        if bars < 52 {
            return 0.0;
        }

        let highs: Vec<f64> = (0..bars).map(|i| data[i * 5 + 1]).collect();
        let lows: Vec<f64> = (0..bars).map(|i| data[i * 5 + 2]).collect();

        // Tenkan-sen (9-period): (highest high + lowest low) / 2
        let tenkan_h = highs[(bars - 9)..bars]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let tenkan_l = lows[(bars - 9)..bars]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let tenkan = (tenkan_h + tenkan_l) / 2.0;

        // Kijun-sen (26-period): (highest high + lowest low) / 2
        let kijun_h = highs[(bars - 26)..bars]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let kijun_l = lows[(bars - 26)..bars]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let kijun = (kijun_h + kijun_l) / 2.0;

        // Senkou Span A = (Tenkan + Kijun) / 2
        let senkou_a = (tenkan + kijun) / 2.0;

        // Senkou Span B (52-period): (highest high + lowest low) / 2
        let senkou_b_h = highs[(bars - 52)..bars]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let senkou_b_l = lows[(bars - 52)..bars]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let _senkou_b = (senkou_b_h + senkou_b_l) / 2.0;

        senkou_a
    }
}
