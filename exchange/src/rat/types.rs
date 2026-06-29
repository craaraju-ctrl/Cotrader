use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── RAT Stream Events ─────────────────────────────────────

/// Top-level event that the RAT stream emits to connected agents.
/// Tagged union so consumers can match on `type` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RatEvent {
    /// Full order book depth snapshot
    OrderbookSnapshot(RatOrderbookSnapshot),
    /// Per-user balance update
    BalanceUpdate(RatBalanceUpdate),
    /// Funding rate tick for a symbol
    FundingTick(RatFundingTick),
    /// Executed trade broadcast
    TradeExecution(RatTradeExecution),
    /// Position change notification
    PositionChange(RatPositionChange),
    /// Heartbeat keepalive
    Heartbeat(RatHeartbeat),
    /// Error / diagnostic message
    Diagnostic(RatDiagnostic),
    /// Agent decision logged to the stream
    AgentDecision(RatAgentDecision),
}

// ── Concrete Payloads ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatOrderbookSnapshot {
    pub symbol: String,
    pub bids: Vec<[f64; 2]>,   // [price, quantity]
    pub asks: Vec<[f64; 2]>,   // [price, quantity]
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatBalanceUpdate {
    pub user_id: String,
    pub asset: String,
    pub available: f64,
    pub locked: f64,
    pub total: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatFundingTick {
    pub symbol: String,
    pub funding_rate: f64,
    pub mark_price: f64,
    pub next_funding_time: DateTime<Utc>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatTradeExecution {
    pub trade_id: Uuid,
    pub symbol: String,
    pub price: f64,
    pub quantity: f64,
    pub total: f64,
    pub buyer_id: String,
    pub seller_id: String,
    pub taker_side: String,   // "Buy" | "Sell"
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatPositionChange {
    pub user_id: String,
    pub symbol: String,
    pub side: String,          // "Long" | "Short"
    pub size: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    pub liquidation_price: f64,
    pub leverage: u32,
    pub margin: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatHeartbeat {
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatDiagnostic {
    pub level: String,         // "info" | "warn" | "error"
    pub message: String,
    pub module: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatAgentDecision {
    pub decision_id: Uuid,
    pub agent_id: String,
    pub symbol: String,
    pub action: String,        // "enter_long" | "enter_short" | "exit_long" | "exit_short" | "hold"
    pub reason: String,
    pub confidence: f64,       // 0.0 .. 1.0
    pub market_snapshot: Option<RatOrderbookSnapshot>,
    pub timestamp: DateTime<Utc>,
}

// ── Agent commands that come back UPSTREAM ─────────────────

/// Commands a RAT agent can send back to the exchange via WS.
#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum RatCommand {
    /// Place an order on behalf of an agent user
    PlaceOrder(RatPlaceOrder),
    /// Cancel an order
    CancelOrder { order_id: Uuid, symbol: String },
    /// Request full state snapshot
    RequestSnapshot { symbols: Option<Vec<String>> },
    /// Update agent configuration
    UpdateConfig { 
        #[serde(default)]
        enabled: Option<bool>,
        #[serde(default)]
        max_position_size: Option<f64>,
        #[serde(default)]
        min_confidence: Option<f64>,
    },
    /// Heartbeat reply
    Pong,
}

#[derive(Debug, Deserialize)]
pub struct RatPlaceOrder {
    pub user_id: String,
    pub symbol: String,
    pub side: String,            // "Buy" | "Sell"
    pub order_type: String,      // "Limit" | "Market" | "StopLoss" | etc.
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub quantity: f64,
}

// ── Auto-detect configuration ─────────────────────────────

/// Connection mode auto-detected by the Memory Agent client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionMode {
    WebSocket,
    Rest,
    Unknown,
}

impl std::fmt::Display for ConnectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionMode::WebSocket => write!(f, "WebSocket"),
            ConnectionMode::Rest => write!(f, "REST"),
            ConnectionMode::Unknown => write!(f, "Unknown"),
        }
    }
}
