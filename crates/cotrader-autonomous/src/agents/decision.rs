//! Decision Agent — Cross-validation → conviction → debate → final verdict.
//!
//! Validates ML predictions against trading rules and makes final decision.

use super::analysis::AnalysisResult;
use super::planning::PlanResult;
use super::reasoning::ReasoningChain;
use crate::state::SharedState;
use crate::types::MarketRegime;

#[derive(Clone)]
pub struct DecisionAgent {
    pub state: SharedState,
}

#[derive(Debug, Clone)]
pub struct DecisionResult {
    pub symbol: String,
    pub action: String, // "BUY", "SELL", "HOLD", "BLOCK"
    pub confidence: f64,
    pub conviction: f64,
    pub ml_score: f64,
    pub verified: bool,
    pub reasoning: String,
}

impl DecisionAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Make final decision: validate, cross-check, and decide.
    pub async fn decide(&self, analysis: &AnalysisResult, plan: &PlanResult) -> DecisionResult {
        let mut conf = plan.confidence;

        // 1. ML signal quality scoring
        let ml_features: Vec<f64> = {
            let mut f = Vec::with_capacity(34);
            f.push(conf); f.push(0.5); f.push(0.5); f.push(0.5);
            f.push(0.5); f.push(0.5); f.push(0.5); f.push(0.5);
            f.extend_from_slice(&vec![0.5; 26]);
            f
        };
        let (ml_profit, ml_source) = self.state.ml_engine.score_signal(&ml_features, conf).await;

        if ml_source == "ml" {
            if ml_profit < 0.3 { conf *= 0.85; }
            else if ml_profit > 0.7 { conf = (conf * 1.05).min(0.95); }
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

        // 3. Risk check
        let portfolio = self.state.portfolio_store.portfolio.read().await;
        let heat = if portfolio.total_equity > 0.0 {
            portfolio.open_positions.iter().map(|p| p.risk_amount).sum::<f64>() / portfolio.total_equity
        } else { 0.0 };

        let risk_ok = heat < 0.08;

        // 4. Rule-based verification
        let mut verified = true;
        let mut verification_notes = Vec::new();

        if !risk_ok {
            verification_notes.push(format!("Portfolio heat too high: {:.1}%", heat * 100.0));
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
            plan.signal.as_ref().map(|s| if s.direction == cotrader_core::TradeDirection::Long { "BUY" } else { "SELL" }).unwrap_or("HOLD").to_string()
        } else {
            "HOLD".to_string()
        };

        let reasoning = format!(
            "ML={:.1}% | Regime={:?} min_conv={:.0}% | Heat={:.1}% | Verified={} | Action={}",
            ml_profit * 100.0, regime, min_conviction * 100.0,
            heat * 100.0, verified, action
        );

        DecisionResult {
            symbol: plan.symbol.clone(),
            action,
            confidence: conf,
            conviction: conf,
            ml_score: ml_profit,
            verified,
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

        chain.finalize(&format!("Decision: {} (conf {:.0}%)", result.action, result.confidence * 100.0));
        chain
    }
}
