//! SkillBridge — AgentSkill implementations for ML models.
//!
//! Each ML model is exposed as an AgentSkill so it integrates naturally
//! into the existing agent hierarchy and can be called from the pipeline.

use crate::engine::MLEngine;
use async_trait::async_trait;
use cotrader_core::skills::AgentSkill;
use cotrader_core::{AgentInput, AgentOutput};
use std::error::Error;
use std::sync::Arc;

/// ML Regime Classifier skill.
pub struct MLRegimeClassifier {
    pub engine: Arc<MLEngine>,
}

#[async_trait]
impl AgentSkill for MLRegimeClassifier {
    fn name(&self) -> &str {
        "MLRegimeClassifier"
    }

    fn description(&self) -> &str {
        "ML-powered market regime classification using neural network. Falls back to threshold detection if model unavailable."
    }

    async fn execute(&self, input: &AgentInput) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        if let AgentInput::ConfluenceRequest { context } = input {
            // Build features from context (simplified — full implementation would pull from SharedState)
            let features = vec![0.5; 30]; // placeholder — real impl builds from MetricsSnapshot
            let fallback = detect_regime_fallback(context.current_price);
            let (regime, confidence, source) = self.engine.predict_regime(&features, fallback).await;

            println!(
                "[ML Skill] {} for {}: {:?} (conf={:.1}%, source={})",
                self.name(), context.symbol, regime, confidence * 100.0, source
            );

            let direction = match regime {
                cotrader_core::MarketRegime::TrendingBull => cotrader_core::agent::SkillDirection::Bullish,
                cotrader_core::MarketRegime::TrendingBear => cotrader_core::agent::SkillDirection::Bearish,
                _ => cotrader_core::agent::SkillDirection::Neutral,
            };

            Ok(AgentOutput::SkillResult {
                name: self.name().to_string(),
                score: confidence,
                note: format!("{:?} ({})", regime, source),
                confidence,
                direction,
                weight: 0.25,
            })
        } else {
            Ok(AgentOutput::Done)
        }
    }
}

/// ML Signal Quality Scorer skill.
pub struct MLSignalScorer {
    pub engine: Arc<MLEngine>,
}

#[async_trait]
impl AgentSkill for MLSignalScorer {
    fn name(&self) -> &str {
        "MLSignalScorer"
    }

    fn description(&self) -> &str {
        "ML-powered signal quality prediction. Predicts probability that a trade signal will be profitable."
    }

    async fn execute(&self, input: &AgentInput) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        if let AgentInput::ConfluenceRequest { context } = input {
            let features = vec![0.5; 34]; // placeholder
            let (probability, source) = self.engine.score_signal(&features, 0.5).await;

            println!(
                "[ML Skill] {} for {}: P(profitable)={:.1}%, source={}",
                self.name(), context.symbol, probability * 100.0, source
            );

            let direction = if probability > 0.6 {
                cotrader_core::agent::SkillDirection::Bullish
            } else if probability < 0.4 {
                cotrader_core::agent::SkillDirection::Bearish
            } else {
                cotrader_core::agent::SkillDirection::Neutral
            };

            Ok(AgentOutput::SkillResult {
                name: self.name().to_string(),
                score: probability,
                note: format!("P(profitable)={:.1}% ({})", probability * 100.0, source),
                confidence: probability,
                direction,
                weight: 0.30,
            })
        } else {
            Ok(AgentOutput::Done)
        }
    }
}

/// ML Win Probability skill (for dynamic Kelly sizing).
pub struct MLWinProbability {
    pub engine: Arc<MLEngine>,
}

#[async_trait]
impl AgentSkill for MLWinProbability {
    fn name(&self) -> &str {
        "MLWinProbability"
    }

    fn description(&self) -> &str {
        "ML-powered win probability prediction for dynamic Kelly Criterion sizing. Replaces hardcoded 0.55."
    }

    async fn execute(&self, input: &AgentInput) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        if let AgentInput::ConfluenceRequest { context } = input {
            let features = vec![0.5; 48]; // placeholder
            let (win_prob, source) = self.engine.predict_win_probability(&features, 0.55).await;

            println!(
                "[ML Skill] {} for {}: P(win)={:.1}%, source={}",
                self.name(), context.symbol, win_prob * 100.0, source
            );

            Ok(AgentOutput::SkillResult {
                name: self.name().to_string(),
                score: win_prob,
                note: format!("P(win)={:.1}% ({})", win_prob * 100.0, source),
                confidence: 0.8,
                direction: cotrader_core::agent::SkillDirection::Neutral,
                weight: 0.20,
            })
        } else {
            Ok(AgentOutput::Done)
        }
    }
}

/// ML Pattern Detector skill.
pub struct MLPatternDetector {
    pub engine: Arc<MLEngine>,
}

#[async_trait]
impl AgentSkill for MLPatternDetector {
    fn name(&self) -> &str {
        "MLPatternDetector"
    }

    fn description(&self) -> &str {
        "CNN-powered pattern detection on raw OHLCV data. Detects complex multi-candle patterns."
    }

    async fn execute(&self, input: &AgentInput) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        if let AgentInput::ConfluenceRequest { context } = input {
            let features = vec![0.0; 100]; // placeholder (20 bars * 5 features)
            let (direction, confidence, source) = self.engine.detect_patterns(&features).await;

            println!(
                "[ML Skill] {} for {}: {} (conf={:.1}%, source={})",
                self.name(), context.symbol, direction, confidence * 100.0, source
            );

            let skill_dir = match direction.as_str() {
                "StrongBullish" | "WeakBullish" => cotrader_core::agent::SkillDirection::Bullish,
                "StrongBearish" | "WeakBearish" => cotrader_core::agent::SkillDirection::Bearish,
                _ => cotrader_core::agent::SkillDirection::Neutral,
            };

            Ok(AgentOutput::SkillResult {
                name: self.name().to_string(),
                score: confidence,
                note: format!("{} ({})", direction, source),
                confidence,
                direction: skill_dir,
                weight: 0.15,
            })
        } else {
            Ok(AgentOutput::Done)
        }
    }
}

/// ML Strategy Selector skill.
pub struct MLStrategySelector {
    pub engine: Arc<MLEngine>,
}

#[async_trait]
impl AgentSkill for MLStrategySelector {
    fn name(&self) -> &str {
        "MLStrategySelector"
    }

    fn description(&self) -> &str {
        "RandomForest-powered strategy selection. Recommends best strategy (Breakout/Pullback/Sweep) per regime."
    }

    async fn execute(&self, input: &AgentInput) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        if let AgentInput::ConfluenceRequest { context } = input {
            let features = vec![0.5; 48]; // placeholder
            let (strategy_idx, confidence, source) = self.engine.select_strategy(&features, 0).await;

            let strategy_name = match strategy_idx {
                0 => "StructureBreakout",
                1 => "TrendPullback",
                2 => "LiquiditySweep",
                _ => "Unknown",
            };

            println!(
                "[ML Skill] {} for {}: {} (conf={:.1}%, source={})",
                self.name(), context.symbol, strategy_name, confidence * 100.0, source
            );

            Ok(AgentOutput::SkillResult {
                name: self.name().to_string(),
                score: confidence,
                note: format!("{} ({})", strategy_name, source),
                confidence,
                direction: cotrader_core::agent::SkillDirection::Neutral,
                weight: 0.15,
            })
        } else {
            Ok(AgentOutput::Done)
        }
    }
}

/// Simple threshold-based regime fallback (existing logic).
fn detect_regime_fallback(_price: f64) -> cotrader_core::MarketRegime {
    // Minimal fallback — real impl would use SharedState OHLCV history
    cotrader_core::MarketRegime::Ranging
}
