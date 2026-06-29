use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A snapshot of state synced to/from the external Memory Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryState {
    pub cycle_id: Uuid,
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub positions: Vec<MemoryPosition>,
    pub recent_decisions: Vec<MemoryDecision>,
    pub balance_snapshot: Vec<MemoryBalance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPosition {
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDecision {
    pub decision_id: Uuid,
    pub action: String,
    pub symbol: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub outcome: String,   // "executed" | "blocked" | "pending"
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBalance {
    pub asset: String,
    pub total: f64,
}

/// Response from a cross-reference query to the memory agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossReferenceResult {
    pub allowed: bool,
    pub confidence: f64,
    pub conflicting_decisions: Vec<MemoryDecision>,
    pub reason: String,
}

impl CrossReferenceResult {
    pub fn allow(reason: &str) -> Self {
        Self {
            allowed: true,
            confidence: 0.9,
            conflicting_decisions: Vec::new(),
            reason: reason.to_string(),
        }
    }

    pub fn block(reason: &str, conflicts: Vec<MemoryDecision>) -> Self {
        Self {
            allowed: false,
            confidence: 0.0,
            conflicting_decisions: conflicts,
            reason: reason.to_string(),
        }
    }
}

/// Endpoint paths for the Memory Agent REST API
pub const MEMORY_SYNC_PATH: &str = "/api/v1/memory/sync";
pub const MEMORY_CROSS_REF_PATH: &str = "/api/v1/memory/cross-reference";
pub const MEMORY_HEALTH_PATH: &str = "/api/v1/memory/health";
