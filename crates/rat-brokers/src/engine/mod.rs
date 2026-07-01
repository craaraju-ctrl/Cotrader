//! Engine — Core tracking logic (Risk gates, sizes, P&L calculations).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Engine {
    positions: Arc<RwLock<HashMap<String, Position>>>,
    orders: Arc<RwLock<Vec<Order>>>,
    balance: Arc<RwLock<Balance>>,
    config: EngineConfig,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub initial_balance: f64,
    pub max_position_pct: f64,
    pub max_daily_loss_pct: f64,
    pub max_drawdown_pct: f64,
    pub commission_pct: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            initial_balance: 100_000.0,
            max_position_pct: 0.05,
            max_daily_loss_pct: 0.03,
            max_drawdown_pct: 0.10,
            commission_pct: 0.001,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub entry_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub price: f64,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct Balance {
    pub total: f64,
    pub available: f64,
    pub margin_used: f64,
    pub unrealized_pnl: f64,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            positions: Arc::new(RwLock::new(HashMap::new())),
            orders: Arc::new(RwLock::new(Vec::new())),
            balance: Arc::new(RwLock::new(Balance {
                total: config.initial_balance,
                available: config.initial_balance,
                margin_used: 0.0,
                unrealized_pnl: 0.0,
            })),
            config,
        }
    }

    /// Check if a trade passes risk gates.
    pub async fn check_risk(&self, symbol: &str, size: f64, price: f64) -> RiskCheckResult {
        let balance = self.balance.read().await;
        let positions = self.positions.read().await;

        let trade_value = size * price;
        let equity = balance.total + balance.unrealized_pnl;

        // Check position size limit
        if trade_value > equity * self.config.max_position_pct {
            return RiskCheckResult {
                passed: false,
                reason: "Exceeds max position size".to_string(),
            };
        }

        // Check total exposure
        let total_exposure: f64 = positions.values().map(|p| p.quantity * p.current_price).sum();
        if (total_exposure + trade_value) > equity * 0.5 {
            return RiskCheckResult {
                passed: false,
                reason: "Total exposure too high".to_string(),
            };
        }

        // Check drawdown
        let drawdown = (equity - balance.total) / balance.total;
        if drawdown > self.config.max_drawdown_pct {
            return RiskCheckResult {
                passed: false,
                reason: "Max drawdown exceeded".to_string(),
            };
        }

        RiskCheckResult {
            passed: true,
            reason: "Risk check passed".to_string(),
        }
    }

    /// Calculate optimal position size.
    pub fn calculate_size(&self, equity: f64, risk_pct: f64, entry: f64, stop_loss: f64) -> f64 {
        let risk_amount = equity * risk_pct;
        let stop_distance = (entry - stop_loss).abs();
        if stop_distance > 0.0 {
            risk_amount / stop_distance
        } else {
            0.0
        }
    }

    /// Update position with new market data.
    pub async fn update_position(&self, symbol: &str, new_price: f64) {
        let mut positions = self.positions.write().await;
        if let Some(pos) = positions.get_mut(symbol) {
            pos.current_price = new_price;
            pos.unrealized_pnl = match pos.side.as_str() {
                "BUY" => (new_price - pos.entry_price) * pos.quantity,
                "SELL" => (pos.entry_price - new_price) * pos.quantity,
                _ => 0.0,
            };
        }
    }

    /// Get current balance.
    pub async fn get_balance(&self) -> Balance {
        self.balance.read().await.clone()
    }

    /// Get all positions.
    pub async fn get_positions(&self) -> Vec<Position> {
        self.positions.read().await.values().cloned().collect()
    }
}

pub struct RiskCheckResult {
    pub passed: bool,
    pub reason: String,
}
