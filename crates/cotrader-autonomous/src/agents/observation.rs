//! Observation Agent — Trade outcomes → performance tracking → simple rule learning.

use super::reasoning::ReasoningChain;
use crate::types::{AgentOutputEvent, CacheFrame};

#[derive(Clone)]
pub struct ObservationAgent {
    pub cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
}

#[derive(Debug, Clone)]
pub struct ObservationSummary {
    pub total_trades: usize,
    pub win_rate: f64,
    pub avg_regret: f64,
    pub recent_outcome: Option<String>,
    pub rules_discovered: usize,
}

impl ObservationAgent {
    pub fn new(cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>) -> Self {
        Self { cot_tx }
    }

    /// Get performance summary from CacheFrame daily stats.
    pub async fn get_summary(&self, frame: &CacheFrame) -> ObservationSummary {
        let stats = &frame.daily_stats;
        let total = stats.winning_trades_today + stats.losing_trades_today;
        let win_rate = if total > 0 {
            stats.winning_trades_today as f64 / total as f64
        } else {
            0.0
        };

        // Emit COT event
        let _ = self
            .cot_tx
            .send(AgentOutputEvent::Cot {
                agent: "Observation".to_string(),
                symbol: "ALL".to_string(),
                action: "OBSERVED".to_string(),
                reason: format!(
                    "Trades: {}, win_rate: {:.1}%, PnL: {:.2}",
                    total, win_rate * 100.0, stats.daily_pnl
                ),
                confidence: win_rate,
            });

        ObservationSummary {
            total_trades: total as usize,
            win_rate,
            avg_regret: 0.0,
            recent_outcome: None,
            rules_discovered: 0,
        }
    }

    /// Produce reasoning chain.
    pub fn reason(&self, summary: &ObservationSummary) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Observation", "ALL");

        chain.add_step(
            &format!(
                "Performance: {} trades, {:.1}% win rate",
                summary.total_trades,
                summary.win_rate * 100.0
            ),
            "Tracked trade outcomes from portfolio stats",
            vec![
                format!("trades={}", summary.total_trades),
                format!("wr={:.1}%", summary.win_rate * 100.0),
            ],
            0.9,
        );

        chain.finalize(&format!(
            "Observed {} trades with {:.1}% win rate",
            summary.total_trades,
            summary.win_rate * 100.0
        ));
        chain
    }
}
