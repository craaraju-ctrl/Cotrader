//! Broker Traits — Universal interface for all broker adapters.

pub mod paper;

use async_trait::async_trait;

/// Universal broker interface that all adapters must implement.
#[async_trait]
pub trait Broker: Send + Sync {
    /// Get broker name.
    fn name(&self) -> &str;

    /// Connect to the broker.
    async fn connect(&mut self) -> Result<(), BrokerError>;

    /// Disconnect from the broker.
    async fn disconnect(&mut self) -> Result<(), BrokerError>;

    /// Check if connected.
    fn is_connected(&self) -> bool;

    /// Place an order.
    async fn place_order(&self, order: NewOrder) -> Result<OrderId, BrokerError>;

    /// Cancel an order.
    async fn cancel_order(&self, order_id: &OrderId) -> Result<(), BrokerError>;

    /// Get open orders.
    async fn get_open_orders(&self, symbol: &str) -> Result<Vec<Order>, BrokerError>;

    /// Get positions.
    async fn get_positions(&self) -> Result<Vec<Position>, BrokerError>;

    /// Get account balance.
    async fn get_balance(&self) -> Result<Balance, BrokerError>;

    /// Get market data.
    async fn get_market_data(&self, symbol: &str) -> Result<MarketData, BrokerError>;

    /// Subscribe to real-time data.
    async fn subscribe(&self, symbols: Vec<String>) -> Result<(), BrokerError>;
}

/// Order ID type.
pub type OrderId = String;

/// New order request.
#[derive(Debug, Clone)]
pub struct NewOrder {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
}

/// Order side.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl std::fmt::Display for OrderSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderSide::Buy => write!(f, "BUY"),
            OrderSide::Sell => write!(f, "SELL"),
        }
    }
}

/// Order type.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
}

/// Order status.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderStatus {
    Pending,
    Filled,
    PartiallyFilled,
    Cancelled,
    Rejected,
}

/// Existing order.
#[derive(Debug, Clone)]
pub struct Order {
    pub id: OrderId,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub price: f64,
    pub status: OrderStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Position.
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: OrderSide,
    pub quantity: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
}

/// Account balance.
#[derive(Debug, Clone)]
pub struct Balance {
    pub total: f64,
    pub available: f64,
    pub margin_used: f64,
    pub unrealized_pnl: f64,
}

/// Market data.
#[derive(Debug, Clone, Default)]
pub struct MarketData {
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub volume: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Broker error type.
#[derive(Debug, Clone)]
pub enum BrokerError {
    ConnectionFailed(String),
    OrderRejected(String),
    InsufficientFunds,
    InvalidOrder(String),
    RateLimited,
    ApiError(String),
    Timeout,
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrokerError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            BrokerError::OrderRejected(msg) => write!(f, "Order rejected: {}", msg),
            BrokerError::InsufficientFunds => write!(f, "Insufficient funds"),
            BrokerError::InvalidOrder(msg) => write!(f, "Invalid order: {}", msg),
            BrokerError::RateLimited => write!(f, "Rate limited"),
            BrokerError::ApiError(msg) => write!(f, "API error: {}", msg),
            BrokerError::Timeout => write!(f, "Timeout"),
        }
    }
}

impl std::error::Error for BrokerError {}
