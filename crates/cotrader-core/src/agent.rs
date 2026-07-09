use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::disciplined_core::{DisciplineCheck, MarketContext, PivotLevels};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTier {
    Main,
    Sub,
    /// High-frequency: 100 ms – 5,000 ms cache TTL.
    DayTrading,
    /// High-frequency: 100 ms – 5,000 ms cache TTL.
    Arbitrage,
    /// Paged holding: 1 hour – 24 hours cache TTL.
    SwingTrading,
    /// Fixed 60-second window for Greek calculations.
    OptionsExpiry,
}

impl AgentTier {
    /// Returns (min_ms, max_ms) cache TTL bounds for this tier.
    pub fn cache_ttl_bounds_ms(&self) -> (u64, u64) {
        match self {
            AgentTier::Main | AgentTier::Sub => (1_000, 5_000),
            AgentTier::DayTrading | AgentTier::Arbitrage => (100, 5_000),
            AgentTier::SwingTrading => (3_600_000, 86_400_000),
            AgentTier::OptionsExpiry => (60_000, 60_000),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentInput {
    PivotRequest { high: f64, low: f64, close: f64 },
    ConfluenceRequest { context: MarketContext },
    RiskRequest { context: MarketContext },
    LogOutcome { key: String, value: String },
    None,
}

/// Directional bias of a skill's signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillDirection {
    Bullish,
    Bearish,
    Neutral,
}

impl SkillDirection {
    /// Returns +1 for Bullish, -1 for Bearish, 0 for Neutral.
    pub fn sign(self) -> i8 {
        match self {
            SkillDirection::Bullish => 1,
            SkillDirection::Bearish => -1,
            SkillDirection::Neutral => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentOutput {
    PivotResult(PivotLevels),
    ConfluenceResult(f64),
    RiskResult(DisciplineCheck),
    SkillResult {
        name: String,
        score: f64,
        note: String,
        confidence: f64,
        /// Directional bias of the skill signal.
        direction: SkillDirection,
        /// Relative importance weight for ensemble aggregation (default 1.0).
        weight: f64,
    },
    Done,
    NoOutput,
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    fn tier(&self) -> AgentTier;
    async fn run(
        &self,
        input: Option<AgentInput>,
    ) -> Result<AgentOutput, Box<dyn Error + Send + Sync>>;
}

impl AgentOutput {
    pub fn is_ok(&self) -> bool {
        match self {
            AgentOutput::RiskResult(check) => check.passed,
            AgentOutput::Done | AgentOutput::NoOutput => true,
            _ => true,
        }
    }

    /// Extract the score from a `SkillResult` variant, if this is one.
    pub fn skill_score(&self) -> Option<f64> {
        match self {
            AgentOutput::SkillResult { score, .. } => Some(*score),
            _ => None,
        }
    }

    /// Extract the full `SkillResult` fields, if this is one.
    pub fn as_skill_result(&self) -> Option<(&str, f64, &str, f64, SkillDirection, f64)> {
        match self {
            AgentOutput::SkillResult {
                name,
                score,
                note,
                confidence,
                direction,
                weight,
            } => Some((name, *score, note, *confidence, *direction, *weight)),
            _ => None,
        }
    }
}
