//! Reasoning types shared across all 8 agents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single step in an agent's reasoning chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// What was done (e.g., "Computed RSI for BTC")
    pub action: String,
    /// Why it was done (e.g., "RSI > 70 indicates overbought")
    pub reasoning: String,
    /// Supporting evidence (e.g., ["RSI=72.3", "MACD histogram=-0.002"])
    pub evidence: Vec<String>,
    /// Confidence in this step (0.0-1.0)
    pub confidence: f64,
}

/// Complete reasoning chain from an agent's decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    /// Which agent produced this reasoning
    pub agent: String,
    /// Symbol being analyzed
    pub symbol: String,
    /// Ordered reasoning steps
    pub steps: Vec<ReasoningStep>,
    /// Final conclusion
    pub conclusion: String,
    /// Overall confidence
    pub confidence: f64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl ReasoningChain {
    pub fn new(agent: &str, symbol: &str) -> Self {
        Self {
            agent: agent.to_string(),
            symbol: symbol.to_string(),
            steps: Vec::new(),
            conclusion: String::new(),
            confidence: 0.0,
            timestamp: Utc::now(),
        }
    }

    pub fn add_step(&mut self, action: &str, reasoning: &str, evidence: Vec<String>, confidence: f64) {
        self.steps.push(ReasoningStep {
            action: action.to_string(),
            reasoning: reasoning.to_string(),
            evidence,
            confidence,
        });
    }

    pub fn finalize(&mut self, conclusion: &str) {
        self.conclusion = conclusion.to_string();
        self.confidence = if self.steps.is_empty() {
            0.0
        } else {
            self.steps.iter().map(|s| s.confidence).sum::<f64>() / self.steps.len() as f64
        };
    }

    /// Format for logging/display.
    pub fn format_for_log(&self) -> String {
        let mut lines = vec![format!("═══ {} Reasoning: {} ═══", self.agent, self.symbol)];
        for (i, step) in self.steps.iter().enumerate() {
            lines.push(format!("  #{}: {}", i + 1, step.action));
            lines.push(format!("    Why: {}", step.reasoning));
            if !step.evidence.is_empty() {
                lines.push(format!("    Evidence: {}", step.evidence.join(", ")));
            }
            lines.push(format!("    Confidence: {:.0}%", step.confidence * 100.0));
        }
        lines.push(format!("  Conclusion: {} (conf {:.0}%)", self.conclusion, self.confidence * 100.0));
        lines.join("\n")
    }
}
