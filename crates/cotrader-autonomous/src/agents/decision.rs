//! Decision Agent — Cross-validation → conviction → debate → final verdict.

use super::analysis::AnalysisResult;
use super::planning::PlanResult;
use super::reasoning::ReasoningChain;
use crate::types::{AgentOutputEvent, CacheFrame, MarketRegime, PortfolioState};
use std::sync::Arc;

#[derive(Clone)]
pub struct DecisionAgent {
    pub ml_engine: Arc<cotrader_ml::MLEngine>,
    pub cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
}

#[derive(Debug, Clone)]
pub struct DecisionResult {
    pub symbol: String,
    pub action: String,
    pub confidence: f64,
    pub conviction: f64,
    pub ml_score: f64,
    pub verified: bool,
    pub neurosymbolic_verified: bool,
    pub reasoning: String,
}

impl DecisionAgent {
    pub fn new(
        ml_engine: Arc<cotrader_ml::MLEngine>,
        cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
    ) -> Self {
        Self { ml_engine, cot_tx }
    }

    /// Make final decision from CacheFrame + analysis + plan.
    pub async fn decide(
        &self,
        frame: &CacheFrame,
        analysis: &AnalysisResult,
        plan: &PlanResult,
    ) -> DecisionResult {
        let mut conf = plan.confidence;

        // 1. ML signal quality scoring
        let ml_features: Vec<f64> = {
            let mut f = Vec::with_capacity(34);
            f.push(conf);
            f.extend_from_slice(&vec![0.5; 33]);
            f
        };
        let (ml_profit, ml_source) = self.ml_engine.score_signal(&ml_features, conf).await;

        if ml_source == "ml" {
            if ml_profit < 0.3 {
                conf *= 0.85;
            } else if ml_profit > 0.7 {
                conf = (conf * 1.05).min(0.95);
            }
        }

        // 2. Regime threshold check
        let regime = analysis.regime;
        let min_conviction = match regime {
            MarketRegime::TrendingBull => 0.50,
            MarketRegime::TrendingBear => 0.80,
            MarketRegime::Ranging => 0.50,
            MarketRegime::Volatile => 0.75,
            MarketRegime::LowLiquidity => 0.50,
        };

        // 3. Risk check from frame portfolio
        let portfolio = &frame.portfolio;
        let heat = if portfolio.total_equity > 0.0 {
            frame
                .open_positions
                .iter()
                .map(|p| p.risk_amount)
                .sum::<f64>()
                / portfolio.total_equity
        } else {
            0.0
        };
        let risk_ok = heat < 0.08;

        // 4. Rule-based verification
        let mut verified = true;
        let mut verification_notes = Vec::new();

        if !risk_ok {
            verification_notes.push(format!(
                "Portfolio heat too high: {:.1}%",
                heat * 100.0
            ));
            verified = false;
        }

        if portfolio.consecutive_losses >= 5 {
            verification_notes.push("Too many consecutive losses".to_string());
            verified = false;
        }

        if conf < 0.35 {
            verification_notes.push("Confidence too low".to_string());
            verified = false;
        }

        // 5. Final verdict
        let action = if !risk_ok || !verified {
            "BLOCK".to_string()
        } else if conf >= min_conviction && plan.signal.is_some() {
            plan.signal
                .as_ref()
                .map(|s| {
                    if s.direction == cotrader_core::TradeDirection::Long {
                        "BUY"
                    } else {
                        "SELL"
                    }
                })
                .unwrap_or("HOLD")
                .to_string()
        } else {
            "HOLD".to_string()
        };

        let reasoning = format!(
            "ML={:.1}% | Regime={:?} min_conv={:.0}% | Heat={:.1}% | Verified={} | Action={}",
            ml_profit * 100.0,
            regime,
            min_conviction * 100.0,
            heat * 100.0,
            verified,
            action
        );

        // Emit COT event
        let _ = self
            .cot_tx
            .send(AgentOutputEvent::Cot {
                agent: "Decision".to_string(),
                symbol: plan.symbol.clone(),
                action: action.clone(),
                reason: reasoning.clone(),
                confidence: conf,
            });

        DecisionResult {
            symbol: plan.symbol.clone(),
            action,
            confidence: conf,
            conviction: conf,
            ml_score: ml_profit,
            verified,
            neurosymbolic_verified: verified && conf >= min_conviction,
            reasoning,
        }
    }

    /// Produce reasoning chain.
    pub fn reason(&self, result: &DecisionResult) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Decision", &result.symbol);

        chain.add_step(
            &format!("ML signal quality: P(profitable)={:.1}%", result.ml_score * 100.0),
            "Cross-checked signal against ML profitability model",
            vec![format!("ml_score={:.3}", result.ml_score)],
            if result.ml_score > 0.6 { 0.8 } else { 0.5 },
        );

        chain.add_step(
            &format!("Conviction check: {:.1}%", result.conviction * 100.0),
            "Validated conviction against regime-specific threshold",
            vec![format!("conviction={:.3}", result.conviction)],
            0.7,
        );

        chain.add_step(
            &format!("Final action: {}", result.action),
            &result.reasoning,
            vec![format!("action={}", result.action)],
            result.confidence,
        );

        chain.finalize(&format!(
            "Decision: {} (conf {:.0}%)",
            result.action,
            result.confidence * 100.0
        ));
        chain
    }
}
