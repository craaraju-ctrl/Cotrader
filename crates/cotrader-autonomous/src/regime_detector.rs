// RegimeDetector helper (new skill/tool)
// HMM-inspired regime detection (vol + slope) for adaptive strategies.
// Research: Boosts robustness like in QuantStart/TradingAgents regime filters.
// Pure helper.

use crate::state::SharedState;
use crate::types::MarketRegime;
use async_trait::async_trait;
use std::error::Error;
use cotrader_core::{skills::AgentSkill, AgentInput, AgentOutput};

#[derive(Debug)]
pub struct RegimeDetector {
    pub state: SharedState,
}

impl RegimeDetector {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    pub async fn detect_regime(&self, symbol: &str, price: f64) -> MarketRegime {
        let history = self.state.market_data.ohlcv_history.read().await;
        let fallback_regime = if let Some(bars) = history.get(symbol) {
            if bars.len() < 20 {
                return MarketRegime::Ranging;
            }
            let recent = &bars[bars.len() - 10..];
            let vol: f64 = recent
                .windows(2)
                .map(|w| (w[1].close - w[0].close).abs() / w[0].close)
                .sum::<f64>()
                / 9.0;
            let slope = (price - bars[bars.len() - 10].close) / bars[bars.len() - 10].close;

            if vol > 0.025 {
                MarketRegime::Volatile
            } else if slope > 0.02 {
                MarketRegime::TrendingBull
            } else if slope < -0.02 {
                MarketRegime::TrendingBear
            } else {
                MarketRegime::Ranging
            }
        } else {
            MarketRegime::Ranging
        };

        // Try ML regime classification — falls back to threshold logic if no model loaded
        let bars_for_features = history.get(symbol).cloned().unwrap_or_default();
        let ml_features = self.state.ml_engine.feature_store().build_features(
            50.0, 0.0, 0.015, 0.0, 0.0, 0.0, 50.0, 25.0, 0.0, -50.0,
            0.0, 50.0, 0.0, 0.0, 50.0, 50.0, "uptrend", 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 50.0, 50.0, 0.0, 0.0, 1.0, 0.02,
            &bars_for_features, Some(fallback_regime), 0.02, 0, 0.0,
        );
        let (ml_regime, ml_conf, ml_source) = self.state.ml_engine.predict_regime(
            &ml_features, fallback_regime,
        ).await;

        if ml_source == "ml" && ml_conf > 0.6 {
            println!(
                "[RegimeDetector] ML regime for {}: {:?} (conf={:.1}%) — fallback was {:?}",
                symbol, ml_regime, ml_conf * 100.0, fallback_regime
            );
            ml_regime
        } else {
            fallback_regime
        }
    }
}

#[async_trait]
impl AgentSkill for RegimeDetector {
    fn name(&self) -> &str {
        "RegimeDetector"
    }
    fn description(&self) -> &str {
        "Detects market regime (TrendingBull/Bear, Ranging, Volatile) using recent vol + price slope (how to adapt strategy to current market state)."
    }

    async fn execute(
        &self,
        input: &AgentInput,
    ) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        if let AgentInput::ConfluenceRequest { context } = input {
            let regime = self
                .detect_regime(&context.symbol, context.current_price)
                .await;
            println!(
                "[Skill] {} executed for {}: regime={:?}",
                self.name(),
                context.symbol,
                regime
            );
            let direction = match regime {
                crate::types::MarketRegime::TrendingBull => {
                    cotrader_core::agent::SkillDirection::Bullish
                }
                crate::types::MarketRegime::TrendingBear => {
                    cotrader_core::agent::SkillDirection::Bearish
                }
                _ => cotrader_core::agent::SkillDirection::Neutral,
            };
            Ok(AgentOutput::SkillResult {
                name: self.name().to_string(),
                score: 0.8, // regime confidence proxy
                note: format!("{:?}", regime),
                confidence: 0.75,
                direction,
                weight: 0.25,
            })
        } else {
            Ok(AgentOutput::Done)
        }
    }
}
