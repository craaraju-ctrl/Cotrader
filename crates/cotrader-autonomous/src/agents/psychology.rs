//! Psychology Agent — Behavioral bias → emotional state → discipline enforcement.

use super::reasoning::ReasoningChain;
use crate::types::{AgentOutputEvent, CacheFrame};

#[derive(Clone)]
pub struct PsychologyAgent {
    pub cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
}

#[derive(Debug, Clone)]
pub struct PsychologyState {
    pub biases_detected: Vec<Bias>,
    pub discipline_score: f64,
    pub adjustments: Vec<String>,
    pub emotional_state: EmotionalState,
}

#[derive(Debug, Clone)]
pub struct Bias {
    pub name: String,
    pub severity: f64,
    pub evidence: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EmotionalState {
    Calm,
    Anxious,
    Excited,
    Fearful,
    Frustrated,
    Overconfident,
}

impl PsychologyAgent {
    pub fn new(cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>) -> Self {
        Self { cot_tx }
    }

    /// Assess psychological state from CacheFrame portfolio data.
    pub async fn assess(&self, frame: &CacheFrame) -> PsychologyState {
        let portfolio = &frame.portfolio;
        let stats = &frame.daily_stats;
        let mut biases = Vec::new();
        let mut adjustments = Vec::new();

        // 1. Revenge Trading Detection
        if portfolio.consecutive_losses >= 3 {
            biases.push(Bias {
                name: "Revenge Trading".to_string(),
                severity: 0.8,
                evidence: format!("{} consecutive losses", portfolio.consecutive_losses),
                recommendation: "Pause trading for 30 minutes. Reduce position size by 50%."
                    .to_string(),
            });
            adjustments.push("Reduce position size 50% — cooling off period".to_string());
        }

        // 2. Overconfidence Detection
        if stats.winning_trades_today >= 5 && stats.losing_trades_today == 0 {
            biases.push(Bias {
                name: "Overconfidence".to_string(),
                severity: 0.6,
                evidence: format!("{} wins, 0 losses today", stats.winning_trades_today),
                recommendation: "Tighten stop losses. Remember: winning streaks end.".to_string(),
            });
            adjustments.push("Tighten stops — protect accumulated profits".to_string());
        }

        // 3. FOMO Detection
        if stats.total_trades_today >= 15 {
            biases.push(Bias {
                name: "FOMO (Fear Of Missing Out)".to_string(),
                severity: 0.7,
                evidence: format!("{} trades today (high frequency)", stats.total_trades_today),
                recommendation: "No new trades for 1 hour. Review if trades are justified."
                    .to_string(),
            });
            adjustments.push("No new trades for 1 hour — FOMO detected".to_string());
        }

        // 4. Loss Aversion Detection
        if stats.daily_pnl < -stats.total_equity * 0.02 {
            biases.push(Bias {
                name: "Loss Aversion".to_string(),
                severity: 0.5,
                evidence: format!(
                    "Daily PnL: {:.2}% (losses feel twice as painful)",
                    stats.daily_pnl / stats.total_equity * 100.0
                ),
                recommendation: "Stick to rules. Don't hold losers hoping for recovery.".to_string(),
            });
        }

        // 5. Recency Bias Detection
        if portfolio.consecutive_losses >= 2 && stats.total_trades_today >= 8 {
            biases.push(Bias {
                name: "Recency Bias".to_string(),
                severity: 0.4,
                evidence: "Recent losses may be disproportionately influencing decisions"
                    .to_string(),
                recommendation: "Review 20+ trade history, not just recent trades.".to_string(),
            });
        }

        // Calculate discipline score
        let discipline = if biases.is_empty() {
            0.95
        } else {
            let avg_severity: f64 =
                biases.iter().map(|b| b.severity).sum::<f64>() / biases.len() as f64;
            (1.0 - avg_severity).max(0.1)
        };

        // Determine emotional state
        let emotional_state = if portfolio.consecutive_losses >= 4 {
            EmotionalState::Frustrated
        } else if stats.winning_trades_today >= 5 && stats.losing_trades_today == 0 {
            EmotionalState::Overconfident
        } else if portfolio.consecutive_losses >= 2 {
            EmotionalState::Anxious
        } else if stats.daily_pnl > stats.total_equity * 0.03 {
            EmotionalState::Excited
        } else if stats.daily_pnl < -stats.total_equity * 0.02 {
            EmotionalState::Fearful
        } else {
            EmotionalState::Calm
        };

        // Emit COT event
        let _ = self
            .cot_tx
            .send(AgentOutputEvent::Cot {
                agent: "Psychology".to_string(),
                symbol: "ALL".to_string(),
                action: if biases.is_empty() {
                    "HEALTHY".to_string()
                } else {
                    "BIAS_DETECTED".to_string()
                },
                reason: format!(
                    "State: {:?}, biases: {}, discipline: {:.0}%",
                    emotional_state,
                    biases.len(),
                    discipline * 100.0
                ),
                confidence: discipline,
            });

        PsychologyState {
            biases_detected: biases,
            discipline_score: discipline,
            adjustments,
            emotional_state,
        }
    }

    /// Produce reasoning chain.
    pub fn reason(&self, state: &PsychologyState) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Psychology", "ALL");

        chain.add_step(
            &format!("Emotional state: {:?}", state.emotional_state),
            &format!("Discipline score: {:.0}%", state.discipline_score * 100.0),
            vec![format!("state={:?}", state.emotional_state)],
            state.discipline_score,
        );

        for bias in &state.biases_detected {
            chain.add_step(
                &format!("Detected: {} (severity {:.0}%)", bias.name, bias.severity * 100.0),
                &bias.recommendation,
                vec![format!("evidence: {}", bias.evidence)],
                1.0 - bias.severity,
            );
        }

        if state.adjustments.is_empty() {
            chain.finalize("No behavioral biases detected — psychological state healthy");
        } else {
            chain.finalize(&format!(
                "{} biases detected — {} adjustments applied",
                state.biases_detected.len(),
                state.adjustments.len()
            ));
        }

        chain
    }
}
