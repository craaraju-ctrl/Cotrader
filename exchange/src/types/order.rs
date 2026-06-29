use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side { Buy, Sell }

impl Side {
    pub fn opposite(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Limit,
    Market,
    StopLoss,
    StopLimit,
    TakeProfit,
    TakeProfitLimit,
    TrailingStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce { Gtc, Ioc, Fok, PostOnly }

impl TimeInForce {
    pub fn is_good_til_cancelled(&self) -> bool { matches!(self, TimeInForce::Gtc) }
    pub fn is_immediate_or_cancel(&self) -> bool { matches!(self, TimeInForce::Ioc) }
    pub fn is_fill_or_kill(&self) -> bool { matches!(self, TimeInForce::Fok) }
    pub fn is_post_only(&self) -> bool { matches!(self, TimeInForce::PostOnly) }
}

impl Default for TimeInForce {
    fn default() -> Self { TimeInForce::Gtc }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus { Pending, Open, Filled, PartiallyFilled, Cancelled, Rejected, Expired }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub user_id: String,
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub status: OrderStatus,
    pub time_in_force: TimeInForce,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // ── Iceberg ─────────────────────────────────────────────
    /// Visible quantity shown on the orderbook (None = standard order, Some = iceberg)
    #[serde(default)]
    pub visible_quantity: Option<f64>,
    // ── Trailing Stop ───────────────────────────────────────
    /// Distance (in price units) the stop trails behind the market
    #[serde(default)]
    pub trailing_delta: Option<f64>,
    /// Current trailing stop price (updated as market moves favorably)
    #[serde(default)]
    pub stop_price: Option<f64>,
    // ── OCO (One-Cancels-Other) ─────────────────────────────
    /// ID of the sibling order in an OCO pair
    #[serde(default)]
    pub oco_sibling_id: Option<Uuid>,
}

impl Order {
    pub fn new_limit(user_id: String, symbol: String, side: Side, price: f64, quantity: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::Limit,
            price: Some(price), trigger_price: None, quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }
    }

    pub fn new_market(user_id: String, symbol: String, side: Side, quantity: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::Market,
            price: None, trigger_price: None, quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }
    }

    pub fn new_stop_loss(user_id: String, symbol: String, side: Side, trigger_price: f64, quantity: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::StopLoss,
            price: None, trigger_price: Some(trigger_price), quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }
    }

    pub fn new_stop_limit(user_id: String, symbol: String, side: Side, trigger_price: f64, limit_price: f64, quantity: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::StopLimit,
            price: Some(limit_price), trigger_price: Some(trigger_price), quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }
    }

    /// TakeProfit BUY triggers when price falls to trigger (buy the dip protection)
    /// TakeProfit SELL triggers when price rises to trigger (take profit on longs)
    pub fn new_take_profit(user_id: String, symbol: String, side: Side, trigger_price: f64, quantity: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::TakeProfit,
            price: None, trigger_price: Some(trigger_price), quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }
    }

    /// TakeProfitLimit: converts to a Limit order when triggered, with the specified limit price
    pub fn new_take_profit_limit(user_id: String, symbol: String, side: Side, trigger_price: f64, limit_price: f64, quantity: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::TakeProfitLimit,
            price: Some(limit_price), trigger_price: Some(trigger_price), quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }
    }

    /// TrailingStop: stop price trails the market by trailing_delta distance
    /// For Sell: stop_price stays below market by delta, rises as market rises
    /// For Buy: stop_price stays above market by delta, falls as market falls
    pub fn new_trailing_stop(user_id: String, symbol: String, side: Side, trailing_delta: f64, quantity: f64, initial_stop_price: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::TrailingStop,
            price: None, trigger_price: Some(initial_stop_price), quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: None, trailing_delta: Some(trailing_delta), stop_price: Some(initial_stop_price), oco_sibling_id: None,
        }
    }

    /// Iceberg limit order: shows `visible_qty` on the book, total `quantity` (including hidden)
    pub fn new_iceberg(user_id: String, symbol: String, side: Side, price: f64, quantity: f64, visible_qty: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), user_id, symbol, side, order_type: OrderType::Limit,
            price: Some(price), trigger_price: None, quantity, filled_quantity: 0.0,
            status: OrderStatus::Pending, time_in_force: TimeInForce::Gtc,
            created_at: now, updated_at: now,
            visible_quantity: Some(visible_qty.min(quantity)), trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }
    }

    pub fn new_post_only(user_id: String, symbol: String, side: Side, price: f64, quantity: f64) -> Self {
        let mut o = Self::new_limit(user_id, symbol, side, price, quantity);
        o.time_in_force = TimeInForce::PostOnly;
        o
    }

    pub fn new_ioc(user_id: String, symbol: String, side: Side, price: f64, quantity: f64) -> Self {
        let mut o = Self::new_limit(user_id, symbol, side, price, quantity);
        o.time_in_force = TimeInForce::Ioc;
        o
    }

    pub fn new_fok(user_id: String, symbol: String, side: Side, price: f64, quantity: f64) -> Self {
        let mut o = Self::new_limit(user_id, symbol, side, price, quantity);
        o.time_in_force = TimeInForce::Fok;
        o
    }

    pub fn remaining_quantity(&self) -> f64 { self.quantity - self.filled_quantity }
    pub fn is_fully_filled(&self) -> bool { self.filled_quantity >= self.quantity }
    pub fn is_active(&self) -> bool { matches!(self.status, OrderStatus::Open | OrderStatus::PartiallyFilled) }

    /// Returns true if this is a conditional/trigger order (stored off-book until triggered)
    pub fn is_trigger_order(&self) -> bool {
        matches!(self.order_type,
            OrderType::StopLoss | OrderType::StopLimit |
            OrderType::TakeProfit | OrderType::TakeProfitLimit |
            OrderType::TrailingStop
        )
    }

    /// Legacy alias for backward compatibility
    pub fn is_stop_order(&self) -> bool { self.is_trigger_order() }

    /// Test if this order is triggered by a given market price.
    /// StopLoss/TrailingStop (Sell): triggers when price FALLS to trigger_price
    /// StopLoss/TrailingStop (Buy):  triggers when price RISES to trigger_price
    /// TakeProfit (Sell): triggers when price RISES to trigger_price
    /// TakeProfit (Buy):  triggers when price FALLS to trigger_price
    pub fn is_triggered_by(&self, market_price: f64) -> bool {
        let tp = match self.order_type {
            OrderType::TakeProfit | OrderType::TakeProfitLimit => {
                match self.side {
                    // Sell TP triggers when price rises (taking profit on a long)
                    Side::Sell => self.trigger_price.map(|tp| market_price >= tp),
                    // Buy TP triggers when price falls (taking profit on a short)
                    Side::Buy => self.trigger_price.map(|tp| market_price <= tp),
                }
            }
            OrderType::TrailingStop => {
                // Trailing stop uses current stop_price (already adjusted by trailing logic)
                self.stop_price.or(self.trigger_price).map(|sp| {
                    match self.side {
                        Side::Sell => market_price <= sp,
                        Side::Buy => market_price >= sp,
                    }
                })
            }
            _ => {
                // StopLoss/StopLimit: standard trigger logic
                match self.side {
                    Side::Buy => self.trigger_price.map(|tp| market_price >= tp),
                    Side::Sell => self.trigger_price.map(|tp| market_price <= tp),
                }
            }
        };
        tp.unwrap_or(false)
    }

    /// Update trailing stop price based on favorable market movement.
    /// Returns true if the price was updated.
    pub fn update_trail(&mut self, market_price: f64) -> bool {
        let delta = match self.trailing_delta {
            Some(d) if d > 0.0 => d,
            _ => return false,
        };
        let current_stop = self.stop_price.or(self.trigger_price).unwrap_or(0.0);
        let new_stop = match self.side {
            // Sell trailing stop: as market rises, stop_price rises to trail at delta below
            Side::Sell => {
                let candidate = market_price - delta;
                if candidate > current_stop { candidate } else { return false; }
            }
            // Buy trailing stop: as market falls, stop_price falls to trail at delta above
            Side::Buy => {
                let candidate = market_price + delta;
                if candidate < current_stop { candidate } else { return false; }
            }
        };
        self.stop_price = Some(new_stop);
        self.trigger_price = Some(new_stop);
        true
    }
}

// ── Trade ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub symbol: String,
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub buyer_id: String,
    pub seller_id: String,
    pub price: f64,
    pub quantity: f64,
    pub total: f64,
    pub taker_side: Side,
    pub timestamp: DateTime<Utc>,
}

impl Trade {
    pub fn new(symbol: String, buy_order_id: Uuid, sell_order_id: Uuid, buyer_id: String, seller_id: String, price: f64, quantity: f64, taker_side: Side) -> Self {
        Self {
            id: Uuid::new_v4(), symbol, buy_order_id, sell_order_id,
            buyer_id, seller_id, price, quantity, total: price * quantity,
            taker_side, timestamp: Utc::now(),
        }
    }
}

// ── Order Book ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel { pub price: f64, pub quantity: f64, pub order_count: u64 }

// ── API Request / Response ─────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PlaceOrderRequest {
    pub user_id: String,
    pub symbol: String,
    pub side: Side,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub quantity: f64,
    #[serde(default)]
    pub time_in_force: TimeInForce,
    // Iceberg
    #[serde(default)]
    pub visible_quantity: Option<f64>,
    // Trailing Stop
    #[serde(default)]
    pub trailing_delta: Option<f64>,
    // OCO
    #[serde(default)]
    pub oco_sibling_id: Option<Uuid>,
}

/// Request to place an OCO (One-Cancels-Other) pair of orders
#[derive(Debug, Deserialize)]
pub struct OcoRequest {
    pub user_id: String,
    pub symbol: String,
    pub side: Side,           // direction of the position
    pub quantity: f64,
    /// Price for the limit (take-profit) leg
    pub price: f64,
    /// Trigger price for the stop-limit (stop-loss) leg
    pub stop_price: f64,
    /// Limit price for the stop-limit leg (optional, defaults to a market-trigger)
    pub stop_limit_price: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct OcoResponse {
    pub oco_id: Uuid,
    pub orders: Vec<PlaceOrderResponse>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PlaceOrderResponse {
    pub order_id: Uuid,
    pub status: OrderStatus,
    pub trades: Vec<Trade>,
    pub message: String,
    pub filled_quantity: f64,
    pub remaining_quantity: f64,
}

#[derive(Debug, Serialize)]
pub struct OrderBookSnapshot {
    pub symbol: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candle {
    pub symbol: String,
    pub interval: String,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub trades: u64,
}

#[derive(Debug, Deserialize)]
pub struct AmendRequest {
    pub order_id: Uuid,
    pub price: Option<f64>,
    pub quantity: Option<f64>,
}

// ── Market Config ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConfig {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub price_precision: u32,
    pub quantity_precision: u32,
    pub min_quantity: f64,
    pub min_notional: f64,
    pub maker_fee: f64,
    pub taker_fee: f64,
    /// Allowed order types for this market (Binance-style)
    pub order_types: Vec<String>,
    /// Status (TRADING, BREAK, HALT)
    pub status: String,
}

impl Default for MarketConfig {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            base_asset: String::new(),
            quote_asset: String::new(),
            price_precision: 2,
            quantity_precision: 4,
            min_quantity: 0.0001,
            min_notional: 1.0,
            maker_fee: 0.001,
            taker_fee: 0.002,
            order_types: vec!["LIMIT".into(), "MARKET".into(), "STOP_LOSS".into(), "STOP_LIMIT".into()],
            status: "TRADING".into(),
        }
    }
}

// ── Exchange Info (Binance-style) ──────────────────────────

#[derive(Debug, Serialize)]
pub struct ExchangeInfo {
    pub timezone: String,
    pub server_time: i64,
    pub symbols: Vec<MarketConfig>,
    pub rate_limits: Vec<RateLimitInfo>,
}

#[derive(Debug, Serialize)]
pub struct RateLimitInfo {
    pub rate_limit_type: String,
    pub interval: String,
    pub interval_num: u32,
    pub limit: u32,
}

// ── 24hr Ticker ────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct Ticker24hr {
    pub symbol: String,
    pub price_change: f64,
    pub price_change_percent: f64,
    pub last_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub volume: f64,
    pub quote_volume: f64,
    pub count: u64,
    pub first_trade_id: String,
    pub last_trade_id: String,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
}

// ── Futures Types ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarginMode { Isolated, Cross }

impl Default for MarginMode { fn default() -> Self { Self::Cross } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionMode { OneWay, Hedge }

impl Default for PositionMode { fn default() -> Self { Self::OneWay } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide { Long, Short }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeverageRequest {
    pub user_id: String,
    pub symbol: String,
    pub leverage: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginModeRequest {
    pub user_id: String,
    pub symbol: String,
    pub margin_mode: MarginMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionModeRequest {
    pub user_id: String,
    pub position_mode: PositionMode,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionInfo {
    pub symbol: String,
    pub side: PositionSide,
    pub size: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    pub pnl_percent: f64,
    pub liquidation_price: f64,
    pub leverage: u32,
    pub margin: f64,
    pub margin_mode: MarginMode,
}

#[derive(Debug, Clone, Serialize)]
pub struct FundingRateInfo {
    pub symbol: String,
    pub funding_rate: f64,
    pub next_funding_time: chrono::DateTime<chrono::Utc>,
    pub last_funding_time: chrono::DateTime<chrono::Utc>,
}

// ── Default Markets ────────────────────────────────────────

fn all_order_types() -> Vec<String> {
    vec![
        "LIMIT".into(), "MARKET".into(), "STOP_LOSS".into(), "STOP_LIMIT".into(),
        "TAKE_PROFIT".into(), "TAKE_PROFIT_LIMIT".into(), "TRAILING_STOP".into(),
    ]
}

pub fn default_markets() -> Vec<MarketConfig> {
    let otypes = all_order_types();
    vec![
        MarketConfig {
            symbol: "BTC/USD".into(),
            base_asset: "BTC".into(),
            quote_asset: "USD".into(),
            price_precision: 2,
            quantity_precision: 6,
            min_quantity: 0.0001,
            min_notional: 10.0,
            maker_fee: 0.001,
            taker_fee: 0.002,
            order_types: otypes.clone(),
            status: "TRADING".into(),
        },
        MarketConfig {
            symbol: "ETH/USD".into(),
            base_asset: "ETH".into(),
            quote_asset: "USD".into(),
            price_precision: 2,
            quantity_precision: 4,
            min_quantity: 0.001,
            min_notional: 5.0,
            maker_fee: 0.001,
            taker_fee: 0.002,
            order_types: otypes.clone(),
            status: "TRADING".into(),
        },
        MarketConfig {
            symbol: "SOL/USD".into(),
            base_asset: "SOL".into(),
            quote_asset: "USD".into(),
            price_precision: 2,
            quantity_precision: 2,
            min_quantity: 0.1,
            min_notional: 1.0,
            maker_fee: 0.001,
            taker_fee: 0.002,
            order_types: otypes.clone(),
            status: "TRADING".into(),
        },
        MarketConfig {
            symbol: "BTC/ETH".into(),
            base_asset: "BTC".into(),
            quote_asset: "ETH".into(),
            price_precision: 4,
            quantity_precision: 6,
            min_quantity: 0.0001,
            min_notional: 0.01,
            maker_fee: 0.001,
            taker_fee: 0.002,
            order_types: otypes.clone(),
            status: "TRADING".into(),
        },
        MarketConfig {
            symbol: "ADA/USD".into(),
            base_asset: "ADA".into(),
            quote_asset: "USD".into(),
            price_precision: 4,
            quantity_precision: 0,
            min_quantity: 1.0,
            min_notional: 1.0,
            maker_fee: 0.001,
            taker_fee: 0.002,
            order_types: otypes.clone(),
            status: "TRADING".into(),
        },
    ]
}
