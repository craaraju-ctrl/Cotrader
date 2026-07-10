//! Unified Multi-Asset Broker Trait
//!
//! This module provides the core async trait and data normalization layer
//! for multi-asset institutional trading across Equity, Crypto, Forex, and Commodity.

use async_trait::async_trait;
use std::error::Error;
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════════════
// Normalized Data Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Asset class enumeration for multi-asset routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetClass {
    Equity,
    Crypto,
    Forex,
    Commodity,
}

impl fmt::Display for AssetClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetClass::Equity => write!(f, "Equity"),
            AssetClass::Crypto => write!(f, "Crypto"),
            AssetClass::Forex => write!(f, "Forex"),
            AssetClass::Commodity => write!(f, "Commodity"),
        }
    }
}

/// Normalized order intent — universal across all broker adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedOrderIntent {
    pub symbol: String,
    pub asset_class: AssetClass,
    pub side: OrderSide,
    pub quantity: f64,
    pub order_type: OrderType,
    pub price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub time_in_force: TimeInForce,
    pub client_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit { price: f64 },
    Stop { stop_price: f64 },
    StopLimit { stop_price: f64, limit_price: f64 },
    TrailingStop { trail_percent: f64 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TimeInForce {
    Day,
    Gtc, // Good Till Cancel
    Ioc, // Immediate or Cancel
    Fok, // Fill or Kill
}

/// Normalized order response — universal across all broker adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub order_id: String,
    pub symbol: String,
    pub status: OrderStatus,
    pub filled_quantity: f64,
    pub filled_price: f64,
    pub commission: f64,
    pub timestamp: String,
    pub raw_response: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    Pending,
    Open,
    Filled,
    PartiallyFilled,
    Cancelled,
    Rejected,
    Expired,
}

/// Normalized position — universal across all broker adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedPosition {
    pub symbol: String,
    pub asset_class: AssetClass,
    pub quantity: f64,
    pub average_entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_percent: f64,
    pub side: OrderSide,
    pub leverage: Option<f64>,
    pub liquidation_price: Option<f64>,
    pub margin_used: Option<f64>,
}

/// Normalized market data — universal across all broker streams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMarketData {
    pub symbol: String,
    pub asset_class: AssetClass,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub volume_24h: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub open_24h: f64,
    pub timestamp: String,
    pub raw_source: String,
}

/// WebSocket event types for stream routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// Price tick update.
    PriceTick(NormalizedMarketData),
    /// Trade execution.
    TradeExecuted(OrderResponse),
    /// Position update.
    PositionUpdate(Vec<NormalizedPosition>),
    /// Order book update.
    OrderBookUpdate {
        symbol: String,
        bids: Vec<(f64, f64)>,
        asks: Vec<(f64, f64)>,
    },
    /// Connection status change.
    ConnectionStatus {
        connected: bool,
        reason: Option<String>,
    },
    /// Health heartbeat.
    HealthHeartbeat {
        broker_id: String,
        timestamp: String,
        uptime_ms: u64,
    },
}

// ═══════════════════════════════════════════════════════════════════════════════
// LiveBrokerAdapter Trait
// ═══════════════════════════════════════════════════════════════════════════════

/// Unified async trait for all broker adapters.
/// Provides uniform signatures for multi-asset institutional trading.
#[async_trait]
pub trait LiveBrokerAdapter: Send + Sync {
    /// Returns the broker identifier.
    fn broker_id(&self) -> &'static str;
    
    /// Returns the supported asset classes.
    fn supported_asset_classes(&self) -> Vec<AssetClass>;
    
    /// Connect to the broker's WebSocket stream.
    async fn connect_websocket(&self, symbols: Vec<String>) -> Result<(), Box<dyn Error + Send + Sync>>;
    
    /// Disconnect from the broker.
    async fn disconnect(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    
    /// Execute a normalized order.
    async fn execute_order(&self, order: NormalizedOrderIntent) -> Result<OrderResponse, Box<dyn Error + Send + Sync>>;
    
    /// Cancel an order.
    async fn cancel_order(&self, order_id: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    
    /// Fetch all positions.
    async fn fetch_positions(&self) -> Result<Vec<NormalizedPosition>, Box<dyn Error + Send + Sync>>;
    
    /// Fetch market data for a symbol.
    async fn fetch_market_data(&self, symbol: &str) -> Result<NormalizedMarketData, Box<dyn Error + Send + Sync>>;
    
    /// Subscribe to stream events.
    async fn subscribe_events(&self) -> Result<tokio::sync::broadcast::Receiver<StreamEvent>, Box<dyn Error + Send + Sync>>;
    
    /// Check broker health.
    async fn health_check(&self) -> Result<bool, Box<dyn Error + Send + Sync>>;
    
    /// Get broker capabilities.
    fn capabilities(&self) -> BrokerCapabilities;
}

/// Broker capabilities description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerCapabilities {
    pub supports_market_orders: bool,
    pub supports_limit_orders: bool,
    pub supports_stop_orders: bool,
    pub supports_trailing_stops: bool,
    pub supports_options: bool,
    pub supports_futures: bool,
    pub supports_forex: bool,
    pub max_symbols: usize,
    pub max_order_size: f64,
    pub min_order_size: f64,
    pub fractional_shares: bool,
}

// ═══════════════════════════════════════════════════════════════════════════════
// BrokerFactory — Dynamic Router
// ═══════════════════════════════════════════════════════════════════════════════

/// Dynamic router that instantiates the correct broker adapter based on AssetClass.
pub struct BrokerFactory {
    /// Registered broker adapters by asset class.
    adapters: std::collections::HashMap<AssetClass, Box<dyn LiveBrokerAdapter>>,
}

impl BrokerFactory {
    /// Create a new broker factory.
    pub fn new() -> Self {
        Self {
            adapters: std::collections::HashMap::new(),
        }
    }
    
    /// Register a broker adapter for an asset class.
    pub fn register(&mut self, asset_class: AssetClass, adapter: Box<dyn LiveBrokerAdapter>) {
        self.adapters.insert(asset_class, adapter);
    }
    
    /// Get the appropriate broker adapter for an asset class.
    pub fn get_adapter(&self, asset_class: &AssetClass) -> Option<&dyn LiveBrokerAdapter> {
        self.adapters.get(asset_class).map(|a| a.as_ref())
    }
    
    /// Connect all registered adapters.
    pub async fn connect_all(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        for (asset_class, adapter) in &self.adapters {
            eprintln!("[BrokerFactory] Connecting {} adapter: {}", asset_class, adapter.broker_id());
            adapter.connect_websocket(vec![]).await?;
        }
        Ok(())
    }
    
    /// Disconnect all registered adapters.
    pub async fn disconnect_all(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        for (asset_class, adapter) in &self.adapters {
            eprintln!("[BrokerFactory] Disconnecting {} adapter: {}", asset_class, adapter.broker_id());
            adapter.disconnect().await?;
        }
        Ok(())
    }
}
