use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

// ── Data Point — a single analyzed market data point ──────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub mid: Option<f64>,
    pub spread: Option<f64>,
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub last_price: Option<f64>,
    pub funding_rate: Option<f64>,
}

// ── Trade Signal — output of the analysis stage ───────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    pub id: Uuid,
    pub symbol: String,
    pub action: SignalAction,
    pub confidence: f64,         // 0.0 .. 1.0
    pub reason: String,
    pub indicators: Vec<Indicator>,
    pub timestamp: DateTime<Utc>,
    pub data_point: DataPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalAction {
    EnterLong,
    EnterShort,
    ExitLong,
    ExitShort,
    Hold,
}

impl std::fmt::Display for SignalAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalAction::EnterLong => write!(f, "enter_long"),
            SignalAction::EnterShort => write!(f, "enter_short"),
            SignalAction::ExitLong => write!(f, "exit_long"),
            SignalAction::ExitShort => write!(f, "exit_short"),
            SignalAction::Hold => write!(f, "hold"),
        }
    }
}

// ── Indicator — a single technical indicator value ────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Indicator {
    pub name: String,
    pub value: f64,
    pub signal: String,   // "bullish" | "bearish" | "neutral"
}

// ── Execution Decision — output of the decide stage ───────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionDecision {
    pub signal_id: Uuid,
    pub action: SignalAction,
    pub symbol: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub confidence: f64,
    pub reason: String,
    pub pre_checked: bool,   // whether memory cross-reference passed
    pub timestamp: DateTime<Utc>,
}

// ── Pipeline Stage Status ─────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageStatus {
    Idle,
    Processing,
    Completed,
    Failed,
    Skipped,
}

// ── Pipeline Metrics ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineMetrics {
    pub total_data_points: u64,
    pub signals_generated: u64,
    pub decisions_executed: u64,
    pub errors: u64,
    pub fallbacks_activated: u64,
    pub avg_processing_ms: f64,
    pub last_cycle: Option<DateTime<Utc>>,
}

// ── Agent Configuration ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub agent_id: String,
    pub enabled: bool,
    pub symbols: Vec<String>,
    pub max_position_size: f64,
    pub min_confidence: f64,       // minimum signal confidence to act (0.0..1.0)
    pub max_positions_per_symbol: u32,
    pub cooldown_seconds: u64,      // minimum time between trades on same symbol
    pub use_memory_cross_reference: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: "orchestra-agent-01".into(),
            enabled: true,
            symbols: vec!["BTC/USD".into(), "ETH/USD".into()],
            max_position_size: 1.0,
            min_confidence: 0.65,
            max_positions_per_symbol: 1,
            cooldown_seconds: 60,
            use_memory_cross_reference: true,
        }
    }
}
