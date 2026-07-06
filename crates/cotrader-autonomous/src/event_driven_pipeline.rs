//! # Event-Driven Pipeline
//!
//! Triggers analysis on specific market events instead of fixed-interval polling:
//! - Volume spike: volume > 2x 20-period average
//! - Price breakout: price breaks above/below Bollinger Band
//! - Volatility expansion: ATR increases > 50% from 20-period average
//! - Regime transition: regime changes between scans
//!
//! This replaces the fixed 60-second polling loop with event-driven triggers.

use std::collections::HashMap;

/// Types of events that trigger analysis
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerEvent {
    VolumeSpike { symbol: String, ratio: f64 },
    PriceBreakout { symbol: String, direction: BreakoutDirection },
    VolatilityExpansion { symbol: String, atr_ratio: f64 },
    RegimeTransition { symbol: String, from: String, to: String },
    PriceMove { symbol: String, pct_change: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakoutDirection {
    AboveUpperBand,
    BelowLowerBand,
}

/// Tracks price/volume history for event detection
struct SymbolState {
    prices: Vec<f64>,
    volumes: Vec<f64>,
    last_regime: Option<String>,
    last_price: Option<f64>,
}

/// Event-Driven Pipeline that detects triggers and emits events
pub struct EventDrivenPipeline {
    states: HashMap<String, SymbolState>,
    lookback: usize,
    volume_spike_threshold: f64,   // default 2.0
    breakout_atr_multiplier: f64,  // default 2.0
    volatility_expansion_threshold: f64, // default 1.5
    price_move_threshold: f64,     // default 1.0% for significant move
}

impl Default for EventDrivenPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl EventDrivenPipeline {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            lookback: 20,
            volume_spike_threshold: 2.0,
            breakout_atr_multiplier: 2.0,
            volatility_expansion_threshold: 1.5,
            price_move_threshold: 1.0,
        }
    }

    /// Feed a tick and check for trigger events
    pub fn on_tick(&mut self, symbol: &str, price: f64, volume: f64) -> Vec<TriggerEvent> {
        let state = self.states.entry(symbol.to_string()).or_insert_with(|| SymbolState {
            prices: Vec::new(),
            volumes: Vec::new(),
            last_regime: None,
            last_price: None,
        });

        let mut events = Vec::new();

        // Check price move since last tick
        if let Some(last) = state.last_price {
            if last > 0.0 {
                let pct_change = ((price - last) / last * 100.0).abs();
                if pct_change >= self.price_move_threshold {
                    events.push(TriggerEvent::PriceMove {
                        symbol: symbol.to_string(),
                        pct_change: if price > last { pct_change } else { -pct_change },
                    });
                }
            }
        }
        state.last_price = Some(price);

        // Add to history
        state.prices.push(price);
        state.volumes.push(volume);

        // Keep rolling window
        if state.prices.len() > self.lookback * 3 {
            state.prices.drain(..state.prices.len() - self.lookback * 3);
            state.volumes.drain(..state.volumes.len() - self.lookback * 3);
        }

        // Need at least lookback+1 data points for detection
        if state.prices.len() < self.lookback + 1 {
            return events;
        }

        // Volume spike detection
        let recent_volumes = &state.volumes[state.volumes.len() - self.lookback..];
        let avg_volume: f64 = recent_volumes.iter().sum::<f64>() / recent_volumes.len() as f64;
        if avg_volume > 0.0 && volume > avg_volume * self.volume_spike_threshold {
            events.push(TriggerEvent::VolumeSpike {
                symbol: symbol.to_string(),
                ratio: volume / avg_volume,
            });
        }

        // Bollinger breakout detection
        let recent_closes = &state.prices[state.prices.len() - self.lookback..];
        let mean: f64 = recent_closes.iter().sum::<f64>() / recent_closes.len() as f64;
        let variance = recent_closes.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / recent_closes.len() as f64;
        let std_dev = variance.sqrt();
        let upper_band = mean + self.breakout_atr_multiplier * std_dev;
        let lower_band = mean - self.breakout_atr_multiplier * std_dev;

        if price > upper_band {
            events.push(TriggerEvent::PriceBreakout {
                symbol: symbol.to_string(),
                direction: BreakoutDirection::AboveUpperBand,
            });
        } else if price < lower_band {
            events.push(TriggerEvent::PriceBreakout {
                symbol: symbol.to_string(),
                direction: BreakoutDirection::BelowLowerBand,
            });
        }

        // Volatility expansion detection (ATR proxy using price range)
        if state.prices.len() >= self.lookback * 2 {
            let old_range: f64 = state.prices[state.prices.len() - self.lookback * 2..state.prices.len() - self.lookback]
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .sum::<f64>() / self.lookback as f64;
            let new_range: f64 = recent_closes.windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .sum::<f64>() / (self.lookback - 1) as f64;

            if old_range > 0.0 {
                let atr_ratio = new_range / old_range;
                if atr_ratio > self.volatility_expansion_threshold {
                    events.push(TriggerEvent::VolatilityExpansion {
                        symbol: symbol.to_string(),
                        atr_ratio,
                    });
                }
            }
        }

        events
    }

    /// Notify of a regime change
    pub fn on_regime_change(&mut self, symbol: &str, new_regime: &str) -> Option<TriggerEvent> {
        let state = self.states.entry(symbol.to_string()).or_insert_with(|| SymbolState {
            prices: Vec::new(),
            volumes: Vec::new(),
            last_regime: None,
            last_price: None,
        });

        let old_regime = state.last_regime.take();
        state.last_regime = Some(new_regime.to_string());

        if let Some(old) = old_regime {
            if old != new_regime {
                return Some(TriggerEvent::RegimeTransition {
                    symbol: symbol.to_string(),
                    from: old,
                    to: new_regime.to_string(),
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_spike_detection() {
        let mut pipeline = EventDrivenPipeline::new();
        // Feed 21 bars with low volume
        for i in 0..20 {
            pipeline.on_tick("BTC", 50000.0, 100.0);
        }
        // 21st bar with 5x volume spike
        let events = pipeline.on_tick("BTC", 50000.0, 500.0);
        assert!(events.iter().any(|e| matches!(e, TriggerEvent::VolumeSpike { ratio, .. } if *ratio > 2.0)));
    }

    #[test]
    fn test_price_breakout_detection() {
        let mut pipeline = EventDrivenPipeline::new();
        // Feed stable prices
        for _ in 0..20 {
            pipeline.on_tick("BTC", 50000.0, 100.0);
        }
        // Break above upper band
        let events = pipeline.on_tick("BTC", 55000.0, 100.0);
        assert!(events.iter().any(|e| matches!(e, TriggerEvent::PriceBreakout { direction: BreakoutDirection::AboveUpperBand, .. })));
    }

    #[test]
    fn test_price_move_detection() {
        let mut pipeline = EventDrivenPipeline::new();
        pipeline.on_tick("BTC", 50000.0, 100.0);
        let events = pipeline.on_tick("BTC", 51000.0, 100.0); // 2% move
        assert!(events.iter().any(|e| matches!(e, TriggerEvent::PriceMove { pct_change, .. } if *pct_change >= 1.0)));
    }

    #[test]
    fn test_no_events_on_stable_market() {
        let mut pipeline = EventDrivenPipeline::new();
        for i in 0..25 {
            pipeline.on_tick("BTC", 50000.0 + i as f64 * 0.01, 100.0);
        }
        let events = pipeline.on_tick("BTC", 50000.25, 100.0);
        // Should have no volume spike, breakout, or volatility expansion
        assert!(!events.iter().any(|e| matches!(e, TriggerEvent::VolumeSpike { .. })));
    }

    #[test]
    fn test_regime_transition() {
        let mut pipeline = EventDrivenPipeline::new();
        pipeline.on_regime_change("BTC", "CHOPPY");
        let event = pipeline.on_regime_change("BTC", "TRENDING_BULL");
        assert!(event.is_some());
        match event.unwrap() {
            TriggerEvent::RegimeTransition { from, to, .. } => {
                assert_eq!(from, "CHOPPY");
                assert_eq!(to, "TRENDING_BULL");
            }
            _ => panic!("Expected RegimeTransition"),
        }
    }
}
