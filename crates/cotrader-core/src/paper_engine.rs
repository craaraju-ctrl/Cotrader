//! # Broker Core — Shared Types & Broker Abstraction
//!
//! Defines the unified `BrokerAdapter` trait and all shared types used
//! by every broker (Alpaca, Zerodha, etc.).
//!
//! The **exact same code path** is used for paper and live trading.
//! The only difference is which API endpoint the broker connects to.
//!
//! ## Architecture
//! ```text
//! StrategyEngine (5-layer pipeline) → TradeSignal
//!     → BrokerRegistry
//!         → AlpacaBroker(paper=true)  → paper-api.alpaca.markets [PAPER MODE]
//!         → AlpacaBroker(paper=false) → api.alpaca.markets      [LIVE MODE]
//! ```
//!
//! Alpaca Markets provides **identical REST endpoints, WebSocket streams,
//! and JSON response formats** for both paper and live — the agent code
//! never changes. Only the base URL and API keys differ.
//!
//! ## Key types
//! - [`BrokerAdapter`] — trait every broker implements
//! - [`BrokerRegistry`] — routes orders between paper/live mode
//! - [`OrderRequest`], [`OrderType`], [`OrderStatus`] — order lifecycle
//! - [`Position`], [`ClosedTrade`] — position & trade tracking
//! - [`PortfolioSummary`], [`RiskCheckResult`] — portfolio & risk reporting

use crate::TradeDirection;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

// ── Position Status ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionStatus {
    Open,
    Closed,
    StoppedOut,
    TakeProfit,
    Cancelled,
}

impl std::fmt::Display for PositionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PositionStatus::Open => write!(f, "OPEN"),
            PositionStatus::Closed => write!(f, "CLOSED"),
            PositionStatus::StoppedOut => write!(f, "STOPPED_OUT"),
            PositionStatus::TakeProfit => write!(f, "TAKE_PROFIT"),
            PositionStatus::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

// Uses `crate::TradeDirection` (from backtest module) to avoid type collision
// within the rat-core crate. Paper/live share the exact same direction type.

// ── Order Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub direction: TradeDirection,
    pub order_type: OrderType,
    pub qty: f64,
    pub price: Option<f64>, // For limit orders
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub strategy: Option<String>,
    pub client_order_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    StopLossLimit,
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderType::Market => write!(f, "MARKET"),
            OrderType::Limit => write!(f, "LIMIT"),
            OrderType::StopLoss => write!(f, "STOP_LOSS"),
            OrderType::StopLossLimit => write!(f, "STOP_LOSS_LIMIT"),
        }
    }
}

// ── Order Status ─────────────────────────────────────────────────────────────

// NOTE: Eq implemented manually because `PartiallyFilled` contains `f64`,
// which does not implement `Eq`. We never construct PartiallyFilled with NaN,
// so the manual impl is safe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    Accepted,
    Filled,
    PartiallyFilled { filled_qty: f64 },
    Rejected { reason: String },
    Cancelled,
    Expired,
}

// SAFETY: We never construct PartiallyFilled with f64::NAN, so reflexivity holds.
// This is needed because f64 doesn't implement Eq but we use Eq in tests and matches.
impl Eq for OrderStatus {}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderStatus::Pending => write!(f, "PENDING"),
            OrderStatus::Accepted => write!(f, "ACCEPTED"),
            OrderStatus::Filled => write!(f, "FILLED"),
            OrderStatus::PartiallyFilled { filled_qty } => {
                write!(f, "PARTIALLY_FILLED (qty={})", filled_qty)
            }
            OrderStatus::Rejected { reason } => write!(f, "REJECTED ({})", reason),
            OrderStatus::Cancelled => write!(f, "CANCELLED"),
            OrderStatus::Expired => write!(f, "EXPIRED"),
        }
    }
}

// ── Position (Open) ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: String,
    pub symbol: String,
    pub direction: TradeDirection,
    pub qty: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_pct: f64,
    pub status: PositionStatus,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub strategy: Option<String>,
    pub order_id: String,
}

// ── Closed Trade (Journal) ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedTrade {
    pub id: String,
    pub symbol: String,
    pub direction: TradeDirection,
    pub qty: f64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub realized_pnl: f64,
    pub realized_pnl_pct: f64,
    pub close_reason: CloseReason,
    pub opened_at: DateTime<Utc>,
    pub closed_at: DateTime<Utc>,
    pub duration_secs: i64,
    pub strategy: Option<String>,
    pub order_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseReason {
    Manual,
    StopLoss,
    TakeProfit,
    Expired,
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloseReason::Manual => write!(f, "MANUAL"),
            CloseReason::StopLoss => write!(f, "STOP_LOSS"),
            CloseReason::TakeProfit => write!(f, "TAKE_PROFIT"),
            CloseReason::Expired => write!(f, "EXPIRED"),
        }
    }
}



// ── Portfolio Summary ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortfolioSummary {
    pub cash: f64,
    pub equity: f64,
    pub margin_used: f64,
    pub free_margin: f64,
    pub daily_pnl: f64,
    pub daily_pnl_pct: f64,
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub win_rate: f64,
    pub consecutive_losses: u32,
    pub max_drawdown: f64,
    pub max_drawdown_pct: f64,
    pub open_positions: usize,
    pub total_pnl_all_time: f64,
}

// ── Risk Check Result ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheckResult {
    pub passed: bool,
    pub max_position_size_ok: bool,
    pub daily_loss_limit_ok: bool,
    pub drawdown_ok: bool,
    pub concentration_ok: bool,
    pub portfolio_heat_ok: bool,
    pub warnings: Vec<String>,
}

// ── Trading Mode ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingMode {
    Paper,
    Live,
}

impl std::fmt::Display for TradingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradingMode::Paper => write!(f, "PAPER"),
            TradingMode::Live => write!(f, "LIVE"),
        }
    }
}



// ── BrokerAdapter Trait ──────────────────────────────────────────────────────

/// Unified interface for ALL broker types (paper AND live).
/// Every broker implementation shares the exact same API.
/// The frontend never knows whether it's talking to paper or live.
#[async_trait::async_trait]
pub trait BrokerAdapter: Send + Sync {
    /// Connect to the broker (authenticate, establish session)
    async fn connect(&self) -> Result<(), String>;

    /// Disconnect gracefully
    async fn disconnect(&self) -> Result<(), String>;

    /// Place an order. Returns the broker's order ID.
    async fn place_order(&self, request: OrderRequest, market_price: f64)
        -> Result<String, String>;

    /// Cancel an open order by ID
    async fn cancel_order(&self, order_id: &str) -> Result<(), String>;

    /// Get all open positions
    async fn get_positions(&self) -> Result<Vec<Position>, String>;

    /// Get portfolio summary
    async fn get_summary(&self) -> Result<PortfolioSummary, String>;

    /// Get order status
    async fn get_order_status(&self, order_id: &str) -> Result<OrderStatus, String>;

    /// Get recent trades
    async fn get_recent_trades(&self, limit: usize) -> Result<Vec<ClosedTrade>, String>;

    /// Update all positions with latest market price. Returns closed trades.
    async fn update_price(
        &self,
        symbol: &str,
        market_price: f64,
    ) -> Result<Vec<ClosedTrade>, String>;

    /// Close a position manually
    async fn close_position(
        &self,
        position_id: &str,
        exit_price: f64,
    ) -> Result<ClosedTrade, String>;

    /// Run risk checks before placing an order
    async fn check_risk(
        &self,
        symbol: &str,
        estimated_cost: f64,
    ) -> Result<RiskCheckResult, String>;

    /// Reset portfolio (paper only — no-op for live)
    async fn reset(&self) -> Result<(), String>;

    /// What mode are we in?
    fn mode(&self) -> TradingMode;

    /// Get a display name for this broker
    fn broker_name(&self) -> &str;
}

// ── BrokerRegistry ───────────────────────────────────────────────────────────

/// Manages broker instances and routes orders to the active one.
/// The frontend talks to the registry, never to individual brokers directly.
pub struct BrokerRegistry {
    paper: Arc<dyn BrokerAdapter>,
    live: RwLock<Option<Arc<dyn BrokerAdapter>>>,
    active_mode: RwLock<TradingMode>,
    active_broker_name: RwLock<String>,
}

impl std::fmt::Debug for BrokerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerRegistry")
            .field("active_mode", &self.active_mode)
            .field("active_broker_name", &self.active_broker_name)
            .finish()
    }
}

impl BrokerRegistry {
    /// Create a new registry with a paper broker adapter.
    ///
    /// The `paper_broker` should be configured for paper trading (e.g.,
    /// `AlpacaBroker::new(key, secret, true)` for Alpaca paper API).
    pub fn new(paper_broker: Arc<dyn BrokerAdapter>) -> Self {
        let name = paper_broker.broker_name().to_string();
        Self {
            paper: paper_broker,
            live: RwLock::new(None),
            active_mode: RwLock::new(TradingMode::Paper),
            active_broker_name: RwLock::new(name),
        }
    }

    /// Register a live broker implementation (replaces any previous).
    pub async fn register_live_broker(&self, broker: Arc<dyn BrokerAdapter>) {
        *self.live.write().await = Some(broker);
    }

    /// Switch trading mode.
    /// - `Paper`: uses the paper broker (always available)
    /// - `Live`: uses the registered live broker (must be registered first)
    pub async fn set_mode(&self, mode: TradingMode) -> Result<(), String> {
        match mode {
            TradingMode::Paper => {
                *self.active_mode.write().await = TradingMode::Paper;
                *self.active_broker_name.write().await = self.paper.broker_name().to_string();
                // Don't auto-connect — let the caller handle connection lifecycle
                Ok(())
            }
            TradingMode::Live => {
                let live = self.live.read().await;
                let broker = live.as_ref().ok_or_else(|| {
                    "No live broker registered. Configure API credentials first.".to_string()
                })?;
                let name = broker.broker_name().to_string();
                drop(live);
                *self.active_mode.write().await = TradingMode::Live;
                *self.active_broker_name.write().await = name;
                Ok(())
            }
        }
    }

    /// Get the currently active broker adapter
    pub async fn active_broker(&self) -> Arc<dyn BrokerAdapter> {
        let mode = self.active_mode.read().await;
        match *mode {
            TradingMode::Paper => self.paper.clone(),
            TradingMode::Live => {
                let live = self.live.read().await;
                live.clone().unwrap_or_else(|| self.paper.clone())
            }
        }
    }

    pub async fn current_mode(&self) -> TradingMode {
        *self.active_mode.read().await
    }

    pub async fn current_broker_name(&self) -> String {
        self.active_broker_name.read().await.clone()
    }

    /// Get a reference to the registered live broker, if any.
    /// Returns `None` if no live broker has been registered.
    /// This is used by the Tredo sync bridge to push paper trades to the
    /// Tredo Exchange even when the system is operating in paper mode.
    pub async fn live_broker(&self) -> Option<Arc<dyn BrokerAdapter>> {
        self.live.read().await.clone()
    }
}
