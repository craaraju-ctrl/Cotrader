//! MarketMaking Strategy
//! Place limit orders on both sides.
//! Profit from bid-ask spread.

use crate::Signal;

pub struct MarketMakingStrategy;

impl MarketMakingStrategy {
    pub fn name() -> &'static str { "MarketMakingStrategy" }

    /// recent_prices: recent mid prices for the asset
    /// recent_volumes: recent volume levels for spread calibration
    pub fn generate_signal(&self, recent_prices: &[f64], recent_volumes: &[f64]) -> Signal {
        if recent_prices.len() < 20 {
            return Signal::hold();
        }

        let price = *recent_prices.last().unwrap();

        // Volume analysis: higher volume = tighter spreads = more confident
        let avg_vol: f64 = if !recent_volumes.is_empty() {
            recent_volumes.iter().sum::<f64>() / recent_volumes.len() as f64
        } else {
            1.0
        };
        let current_vol = recent_volumes.last().copied().unwrap_or(1.0);
        let vol_ratio = if avg_vol > 0.0 { current_vol / avg_vol } else { 1.0 };

        // Determine if market is trending or ranging
        let sma_short = sma(recent_prices, 5);
        let sma_long = sma(recent_prices, 20);
        let trend_strength = ((sma_short - sma_long) / sma_long).abs();

        // Market making: profit from spread
        // BUY when price is below mid (bid side), SELL when above (ask side)
        let mid = sma_long;
        let deviation = (price - mid) / mid;

        if deviation < -0.001 && trend_strength < 0.01 {
            // Price below mid in ranging market — place bid (BUY)
            let confidence = (0.4 + vol_ratio * 0.1 + (1.0 - trend_strength) * 0.2).min(0.8);
            return Signal::buy(confidence);
        }

        if deviation > 0.001 && trend_strength < 0.01 {
            // Price above mid in ranging market — place ask (SELL)
            let confidence = (0.4 + vol_ratio * 0.1 + (1.0 - trend_strength) * 0.2).min(0.8);
            return Signal::sell(confidence);
        }

        Signal::hold()
    }
}

fn sma(data: &[f64], period: usize) -> f64 {
    let len = data.len().min(period);
    if len == 0 { return 0.0; }
    let slice = &data[data.len() - len..];
    slice.iter().sum::<f64>() / len as f64
}
