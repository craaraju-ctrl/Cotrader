//! # LiveOrderManager — SQLite-Backed Order Lifecycle Tracker
//!
//! Persists every live order placed through the broker to SQLite for crash recovery,
//! fill confirmation, rejection tracking, and order-level audit trail.
//!
//! ## Architecture
//! ```text
//! ExecutionCoordinator → broker.place_order() → LiveOrderManager.register_order()
//!                                                      ↓
//! Backgound poll loop → LiveOrderManager.poll_pending() → broker.get_order_status()
//!                                                      ↓
//!                      Filled → update portfolio + COT  |  Rejected → log rejection
//! ```
//!
//! ## Crash Recovery
//! On restart, `get_pending_orders()` returns all orders still in PENDING state.
//! The poll loop re-polls them and reconciles with the broker.

use chrono::Utc;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;
use cotrader_core::paper_engine::{OrderStatus, OrderType};
use cotrader_core::TradeDirection;

// ── Database Path ────────────────────────────────────────────────────────────

/// Returns the default orders DB path from StorageConfig.
fn default_db_path() -> String {
    cotrader_core::StorageConfig::default().orders_db().to_string_lossy().to_string()
}

// ── OrderRecord — Persistent Order State ─────────────────────────────────────

/// A single order tracked through its lifecycle.
/// NOTE: `qty` and `filled_qty` are `f64` to support fractional quantities
/// for crypto assets (e.g., 0.062 BTC). The original `i32` design caused
/// severe oversizing (0.062 BTC → 1 BTC = 16× intended size).
#[derive(Debug, Clone)]
pub struct OrderRecord {
    pub broker_order_id: String,
    pub symbol: String,
    pub direction: TradeDirection,
    pub qty: f64,
    pub order_type: OrderType,
    pub price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub strategy_tag: Option<String>,
    pub status: OrderStatus,
    pub filled_qty: f64,
    pub filled_avg_price: Option<f64>,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl OrderRecord {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Expired
        ) || matches!(self.status, OrderStatus::Rejected { .. })
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.status, OrderStatus::Pending | OrderStatus::Accepted)
            || matches!(self.status, OrderStatus::PartiallyFilled { .. })
    }
}

// ── RejectionStats — Aggregated Rejection Counter ────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct RejectionStats {
    /// Number of consecutive rejections in the current session
    pub consecutive_rejections: u32,
    /// Total rejections today
    pub total_rejections_today: u32,
    /// Most recent rejection reason
    pub last_rejection_reason: String,
    /// Timestamp of last rejection
    pub last_rejection_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Rejection reasons broken down by type
    pub breakdown: std::collections::HashMap<String, u32>,
}

// ── LiveOrderManager ─────────────────────────────────────────────────────────

pub struct LiveOrderManager {
    db: Arc<Mutex<Connection>>,
    rejection_stats: tokio::sync::RwLock<RejectionStats>,
}

impl std::fmt::Debug for LiveOrderManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveOrderManager").finish()
    }
}

impl LiveOrderManager {
    /// Open or create the SQLite database.
    pub fn open(db_path: Option<&str>) -> Result<Self, rusqlite::Error> {
        let default_path = default_db_path();
        let path = db_path.unwrap_or(&default_path);
        let conn = Connection::open(path)?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;",
        )?;

        conn.execute_batch(
            "            CREATE TABLE IF NOT EXISTS live_orders (
                broker_order_id TEXT PRIMARY KEY,
                symbol TEXT NOT NULL,
                direction TEXT NOT NULL,
                qty REAL NOT NULL,
                order_type TEXT NOT NULL,
                price REAL,
                stop_loss REAL,
                take_profit REAL,
                strategy_tag TEXT,
                status TEXT NOT NULL DEFAULT 'PENDING',
                filled_qty REAL DEFAULT 0,
                filled_avg_price REAL,
                error_message TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_live_orders_status ON live_orders(status);
            CREATE INDEX IF NOT EXISTS idx_live_orders_symbol ON live_orders(symbol);
            CREATE INDEX IF NOT EXISTS idx_live_orders_created ON live_orders(created_at);",
        )?;

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            rejection_stats: tokio::sync::RwLock::new(RejectionStats::default()),
        })
    }

    /// Register a newly-placed order in the tracker.
    ///
    /// CRITICAL FIX (fractional qty): qty is now `f64` throughout to support
    /// crypto fractional sizing (e.g. 0.062 BTC). The old `i32` schema caused
    /// every fractional order to round to 1 unit, producing 16× oversizing.
    /// See docs/FIXES.md for the full audit trail.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_order(
        &self,
        broker_order_id: &str,
        symbol: &str,
        direction: TradeDirection,
        qty: f64,
        order_type: OrderType,
        price: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        strategy_tag: Option<String>,
    ) -> Result<(), rusqlite::Error> {
        let now = Utc::now().to_rfc3339();
        let dir_str = match direction {
            TradeDirection::Long => "BUY",
            TradeDirection::Short => "SELL",
        };
        let ot_str = match order_type {
            OrderType::Market => "MARKET",
            OrderType::Limit => "LIMIT",
            OrderType::StopLoss => "SL",
            OrderType::StopLossLimit => "SL-M",
        };

        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO live_orders (broker_order_id, symbol, direction, qty, order_type, price, stop_loss, take_profit, strategy_tag, status, filled_qty, filled_avg_price, error_message, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'PENDING', 0, NULL, NULL, ?10, ?10)",
            rusqlite::params![
                broker_order_id, symbol, dir_str, qty, ot_str,
                price, stop_loss, take_profit, strategy_tag, now
            ],
        )?;

        println!(
            "[LiveOrderManager] 📝 Registered order: {} {} {} qty={}",
            broker_order_id, symbol, dir_str, qty
        );

        Ok(())
    }

    /// Update an order's status after polling the broker.
    pub async fn update_status(
        &self,
        broker_order_id: &str,
        status: OrderStatus,
        filled_qty: f64,
        filled_avg_price: Option<f64>,
        error_message: Option<String>,
    ) -> Result<(), rusqlite::Error> {
        let now = Utc::now().to_rfc3339();
        let status_str = order_status_to_string(&status);

        let db = self.db.lock().await;
        db.execute(
            "UPDATE live_orders SET status=?1, filled_qty=?2, filled_avg_price=?3, error_message=?4, updated_at=?5 WHERE broker_order_id=?6",
            rusqlite::params![status_str, filled_qty, filled_avg_price, error_message, now, broker_order_id],
        )?;

        // Update rejection stats
        if matches!(status, OrderStatus::Rejected { .. }) {
            let mut stats = self.rejection_stats.write().await;
            stats.consecutive_rejections += 1;
            stats.total_rejections_today += 1;
            stats.last_rejection_reason = error_message.clone().unwrap_or_default();
            stats.last_rejection_at = Some(Utc::now());
            let reason_key = error_message
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            *stats.breakdown.entry(reason_key).or_insert(0) += 1;
        } else if status == OrderStatus::Filled {
            // Reset consecutive rejections on successful fill
            let mut stats = self.rejection_stats.write().await;
            stats.consecutive_rejections = 0;
        }

        Ok(())
    }

    /// Get all pending orders that need status polling.
    pub async fn get_pending_orders(&self) -> Result<Vec<OrderRecord>, rusqlite::Error> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT broker_order_id, symbol, direction, qty, order_type, price, stop_loss, take_profit, strategy_tag, status, filled_qty, filled_avg_price, error_message, created_at, updated_at
             FROM live_orders
             WHERE status IN ('PENDING', 'ACCEPTED', 'PARTIALLY_FILLED')
             ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let status_str: String = row.get(9)?;
            Ok(OrderRecord {
                broker_order_id: row.get(0)?,
                symbol: row.get(1)?,
                direction: if row.get::<_, String>(2)? == "BUY" {
                    TradeDirection::Long
                } else {
                    TradeDirection::Short
                },
                qty: row.get::<_, f64>(3)?,
                order_type: parse_order_type(&row.get::<_, String>(4)?),
                price: row.get(5)?,
                stop_loss: row.get(6)?,
                take_profit: row.get(7)?,
                strategy_tag: row.get(8)?,
                status: parse_order_status(
                    &status_str,
                    row.get::<_, f64>(10)?,
                    row.get::<_, f64>(3)?,
                ),
                filled_qty: row.get::<_, f64>(10)?,
                filled_avg_price: row.get(11)?,
                error_message: row.get(12)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(13)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(14)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;

        let mut orders = Vec::new();
        for order in rows.flatten() {
            orders.push(order);
        }
        Ok(orders)
    }

    /// Get all orders for a given symbol within a time window.
    pub async fn get_orders_for_symbol(
        &self,
        symbol: &str,
        since_hours: i64,
    ) -> Result<Vec<OrderRecord>, rusqlite::Error> {
        let cutoff = (Utc::now() - chrono::Duration::hours(since_hours)).to_rfc3339();
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT broker_order_id, symbol, direction, qty, order_type, price, stop_loss, take_profit, strategy_tag, status, filled_qty, filled_avg_price, error_message, created_at, updated_at
             FROM live_orders
             WHERE symbol=?1 AND created_at >= ?2
             ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map(rusqlite::params![symbol, cutoff], |row| {
            let status_str: String = row.get(9)?;
            Ok(OrderRecord {
                broker_order_id: row.get(0)?,
                symbol: row.get(1)?,
                direction: if row.get::<_, String>(2)? == "BUY" {
                    TradeDirection::Long
                } else {
                    TradeDirection::Short
                },
                qty: row.get::<_, f64>(3)?,
                order_type: parse_order_type(&row.get::<_, String>(4)?),
                price: row.get(5)?,
                stop_loss: row.get(6)?,
                take_profit: row.get(7)?,
                strategy_tag: row.get(8)?,
                status: parse_order_status(
                    &status_str,
                    row.get::<_, f64>(10)?,
                    row.get::<_, f64>(3)?,
                ),
                filled_qty: row.get::<_, f64>(10)?,
                filled_avg_price: row.get(11)?,
                error_message: row.get(12)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(13)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(14)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;

        let mut orders = Vec::new();
        for order in rows.flatten() {
            orders.push(order);
        }
        Ok(orders)
    }

    /// Get rejection statistics for circuit breaker decisions.
    pub async fn get_rejection_stats(&self) -> RejectionStats {
        self.rejection_stats.read().await.clone()
    }

    /// Reset rejection statistics (after a cool-down period).
    pub async fn reset_rejection_stats(&self) {
        let mut stats = self.rejection_stats.write().await;
        *stats = RejectionStats::default();
    }

    /// Get the count of orders that reached a terminal state today.
    pub async fn todays_terminal_count(&self) -> Result<u32, rusqlite::Error> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let db = self.db.lock().await;
        let count: u32 = db.query_row(
            "SELECT COUNT(*) FROM live_orders WHERE created_at >= ?1 AND status IN ('FILLED', 'CANCELLED', 'REJECTED', 'EXPIRED')",
            rusqlite::params![today],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Clean up old orders beyond retention period.
    pub async fn prune_old_orders(&self, retention_days: i64) -> Result<u32, rusqlite::Error> {
        let cutoff = (Utc::now() - chrono::Duration::days(retention_days)).to_rfc3339();
        let db = self.db.lock().await;
        let deleted = db.execute(
            "DELETE FROM live_orders WHERE created_at < ?1",
            rusqlite::params![cutoff],
        )?;
        Ok(deleted as u32)
    }
}

// ── Helper Conversions ───────────────────────────────────────────────────────

fn order_status_to_string(status: &OrderStatus) -> String {
    match status {
        OrderStatus::Pending => "PENDING".to_string(),
        OrderStatus::Accepted => "ACCEPTED".to_string(),
        OrderStatus::Filled => "FILLED".to_string(),
        OrderStatus::PartiallyFilled { .. } => "PARTIALLY_FILLED".to_string(),
        OrderStatus::Rejected { .. } => "REJECTED".to_string(),
        OrderStatus::Cancelled => "CANCELLED".to_string(),
        OrderStatus::Expired => "EXPIRED".to_string(),
    }
}

fn parse_order_type(s: &str) -> OrderType {
    match s {
        "MARKET" => OrderType::Market,
        "LIMIT" => OrderType::Limit,
        "SL" => OrderType::StopLoss,
        "SL-M" => OrderType::StopLossLimit,
        _ => OrderType::Market,
    }
}

fn parse_order_status(s: &str, filled_qty: f64, _total_qty: f64) -> OrderStatus {
    match s {
        "PENDING" | "ACCEPTED" => OrderStatus::Pending,
        // NOTE: filled_qty is f64 (supports fractional crypto quantities).
        // The actual filled amount is tracked in OrderRecord.filled_qty.
        "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled { filled_qty },
        "FILLED" => OrderStatus::Filled,
        "CANCELLED" | "EXPIRED" => OrderStatus::Cancelled,
        "REJECTED" => OrderStatus::Rejected {
            reason: "Order rejected by broker".to_string(),
        },
        _ => OrderStatus::Pending,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_query_order() {
        let manager = LiveOrderManager::open(Some(":memory:")).unwrap();

        manager
            .register_order(
                "ORD-001",
                "BTC",
                TradeDirection::Long,
                1.0,
                OrderType::Market,
                Some(50000.0),
                Some(49000.0),
                Some(51000.0),
                Some("test".to_string()),
            )
            .await
            .unwrap();

        let pending = manager.get_pending_orders().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].broker_order_id, "ORD-001");
        assert_eq!(pending[0].symbol, "BTC");
        assert_eq!(pending[0].qty, 1.0);
    }

    #[tokio::test]
    async fn test_update_status_to_filled() {
        let manager = LiveOrderManager::open(Some(":memory:")).unwrap();

        manager
            .register_order(
                "ORD-002",
                "ETH",
                TradeDirection::Long,
                2.0,
                OrderType::Market,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        manager
            .update_status("ORD-002", OrderStatus::Filled, 2.0, Some(3000.0), None)
            .await
            .unwrap();

        let pending = manager.get_pending_orders().await.unwrap();
        assert_eq!(pending.len(), 0, "Filled orders should not be pending");
    }

    #[tokio::test]
    async fn test_update_status_to_rejected() {
        let manager = LiveOrderManager::open(Some(":memory:")).unwrap();

        manager
            .register_order(
                "ORD-003",
                "SOL",
                TradeDirection::Short,
                5.0,
                OrderType::Limit,
                Some(150.0),
                None,
                None,
                Some("test".to_string()),
            )
            .await
            .unwrap();

        manager
            .update_status(
                "ORD-003",
                OrderStatus::Rejected {
                    reason: "Insufficient balance".to_string(),
                },
                0.0,
                None,
                Some("Insufficient balance".to_string()),
            )
            .await
            .unwrap();

        let stats = manager.get_rejection_stats().await;
        assert_eq!(stats.consecutive_rejections, 1);
        assert_eq!(stats.last_rejection_reason, "Insufficient balance");
    }

    #[tokio::test]
    async fn test_consecutive_rejections_reset_on_fill() {
        let manager = LiveOrderManager::open(Some(":memory:")).unwrap();

        // Two rejections
        for i in 0..2 {
            manager
                .register_order(
                    &format!("ORD-R{}", i),
                    "BTC",
                    TradeDirection::Long,
                    1.0,
                    OrderType::Market,
                    None,
                    None,
                    None,
                    None,
                )
                .await
                .unwrap();
            manager
                .update_status(
                    &format!("ORD-R{}", i),
                    OrderStatus::Rejected {
                        reason: "risk_check".to_string(),
                    },
                    0.0,
                    None,
                    Some("risk_check".to_string()),
                )
                .await
                .unwrap();
        }

        let stats = manager.get_rejection_stats().await;
        assert_eq!(stats.consecutive_rejections, 2);

        // Successful fill resets counter
        manager
            .register_order(
                "ORD-FILL",
                "BTC",
                TradeDirection::Long,
                1.0,
                OrderType::Market,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        manager
            .update_status("ORD-FILL", OrderStatus::Filled, 1.0, Some(50000.0), None)
            .await
            .unwrap();

        let stats = manager.get_rejection_stats().await;
        assert_eq!(stats.consecutive_rejections, 0);
    }

    #[tokio::test]
    async fn test_get_orders_for_symbol() {
        let manager = LiveOrderManager::open(Some(":memory:")).unwrap();

        manager
            .register_order(
                "ORD-1",
                "BTC",
                TradeDirection::Long,
                1.0,
                OrderType::Market,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        manager
            .register_order(
                "ORD-2",
                "ETH",
                TradeDirection::Long,
                1.0,
                OrderType::Market,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let btc_orders = manager.get_orders_for_symbol("BTC", 24).await.unwrap();
        assert_eq!(btc_orders.len(), 1);
        assert_eq!(btc_orders[0].broker_order_id, "ORD-1");
    }

    #[tokio::test]
    async fn test_fractional_qty_crypto() {
        // CRITICAL TEST: Crypto fractional quantities (e.g., 0.062 BTC)
        // The old `i32` schema rounded this to 1 BTC = 16× oversizing.
        // The f64 fix ensures fractional qty is stored and retrieved accurately.
        let manager = LiveOrderManager::open(Some(":memory:")).unwrap();

        // Register a fractional order like 0.062 BTC
        manager
            .register_order(
                "ORD-FRAC-001",
                "BTC",
                TradeDirection::Long,
                0.062, // fractional crypto qty
                OrderType::Market,
                Some(65000.0),
                Some(64000.0),
                Some(68000.0),
                Some("fractional_test".to_string()),
            )
            .await
            .unwrap();

        let pending = manager.get_pending_orders().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].broker_order_id, "ORD-FRAC-001");
        assert!(
            (pending[0].qty - 0.062).abs() < 1e-10,
            "Fractional qty must be preserved exactly. Expected 0.062, got {}",
            pending[0].qty
        );
        assert!(
            pending[0].qty < 0.5,
            "Fractional qty {} must remain fractional, not rounded up to 1",
            pending[0].qty
        );

        // Update with fractional fill
        manager
            .update_status("ORD-FRAC-001", OrderStatus::Filled, 0.062, Some(65000.0), None)
            .await
            .unwrap();

        // Verify the order is no longer pending (terminal state)
        let all_orders = manager.get_orders_for_symbol("BTC", 24).await.unwrap();
        assert!(!all_orders.is_empty());
        let order = &all_orders[0];
        assert!(
            (order.filled_qty - 0.062).abs() < 1e-10,
            "Filled fractional qty must be preserved. Expected 0.062, got {}",
            order.filled_qty
        );
    }

    #[tokio::test]
    async fn test_prune_old_orders() {
        let manager = LiveOrderManager::open(Some(":memory:")).unwrap();

        manager
            .register_order(
                "OLD",
                "BTC",
                TradeDirection::Long,
                1.0,
                OrderType::Market,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        manager
            .register_order(
                "NEW",
                "ETH",
                TradeDirection::Long,
                1.0,
                OrderType::Market,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        // Prune with 0-day retention should delete both
        let deleted = manager.prune_old_orders(0).await.unwrap();
        assert_eq!(deleted, 2);
    }
}
