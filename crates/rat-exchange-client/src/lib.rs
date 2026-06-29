//! Rat Exchange API Client
//!
//! Connects to Rat Exchange (port 8080) for:
//! - Real-time market data (orderbook, candles, ticker)
//! - Order placement and management
//! - Portfolio and balance queries
//! - WebSocket streaming

use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

// ═══════════════════════════════════════════════════════════════════════════════
// Errors
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: status={status} message={message}")]
    Api { status: u16, message: String },

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

// ═══════════════════════════════════════════════════════════════════════════════
// Request/Response Types
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceOrderRequest {
    pub user_id: String,
    pub symbol: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub price: Option<f64>,
    pub quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceOrderResponse {
    pub order_id: String,
    pub status: String,
    #[serde(default)]
    pub trades: Vec<Trade>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub filled_quantity: f64,
    #[serde(default)]
    pub remaining_quantity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub user_id: String,
    pub symbol: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub price: f64,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub quantity: f64,
    pub side: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: f64,
    pub quantity: f64,
    pub order_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub open_time: String,
    pub close_time: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub trades: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker24h {
    pub symbol: String,
    pub last_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub volume: f64,
    pub quote_volume: f64,
    pub price_change: f64,
    pub price_change_percent: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub available: f64,
    pub locked: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub user_id: String,
    pub balances: Vec<Balance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    pub pnl_percent: f64,
    pub leverage: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConfig {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// WebSocket Events
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    #[serde(rename = "trade")]
    Trade {
        symbol: String,
        price: f64,
        quantity: f64,
        side: String,
        timestamp: String,
    },
    #[serde(rename = "orderbook_update")]
    OrderBookUpdate {
        symbol: String,
        bids: Vec<[f64; 2]>,
        asks: Vec<[f64; 2]>,
    },
    #[serde(rename = "order_update")]
    OrderUpdate {
        order_id: String,
        status: String,
        filled_quantity: f64,
    },
    #[serde(rename = "depth_update")]
    DepthUpdate {
        symbol: String,
        bids: Vec<[f64; 2]>,
        asks: Vec<[f64; 2]>,
    },
    #[serde(rename = "pong")]
    Pong,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Client
// ═══════════════════════════════════════════════════════════════════════════════

pub struct RatExchangeClient {
    base_url: String,
    api_key: String,
    secret_key: String,
    user_id: String,
    http: reqwest::Client,
}

impl RatExchangeClient {
    pub fn new(base_url: &str, api_key: &str, secret_key: &str, user_id: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            secret_key: secret_key.to_string(),
            user_id: user_id.to_string(),
            http,
        }
    }

    /// Generate HMAC-SHA256 signature for authenticated requests
    fn sign(&self, method: &str, path: &str, nonce: &str) -> String {
        let message = format!("{}{}{}", method, path, nonce);
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Build authenticated headers
    fn auth_headers(&self, method: &str, path: &str) -> HashMap<String, String> {
        let nonce = chrono::Utc::now().timestamp_millis().to_string();
        let signature = self.sign(method, path, &nonce);
        let mut headers = HashMap::new();
        headers.insert("X-API-Key".to_string(), self.api_key.clone());
        headers.insert("X-Signature".to_string(), signature);
        headers.insert("X-Nonce".to_string(), nonce);
        headers
    }

    // ── Public Endpoints ──────────────────────────────────────────────────

    pub async fn health(&self) -> Result<serde_json::Value, ExchangeError> {
        let resp = self.http.get(format!("{}/api/v1/health", self.base_url))
            .send().await?
            .json().await?;
        Ok(resp)
    }

    pub async fn markets(&self) -> Result<Vec<String>, ExchangeError> {
        let resp: serde_json::Value = self.http.get(format!("{}/api/v1/markets", self.base_url))
            .send().await?
            .json().await?;
        let symbols = resp["symbols"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        Ok(symbols)
    }

    pub async fn orderbook(&self, symbol: &str, depth: Option<u32>) -> Result<OrderBook, ExchangeError> {
        let depth = depth.unwrap_or(10);
        let url = format!("{}/api/v1/orderbook?symbol={}&depth={}", self.base_url, symbol, depth);
        let resp = self.http.get(&url).send().await?.json().await?;
        Ok(resp)
    }

    pub async fn candles(&self, symbol: &str, interval: &str, limit: Option<u32>) -> Result<Vec<Candle>, ExchangeError> {
        let limit = limit.unwrap_or(100);
        let url = format!("{}/api/v1/candles?symbol={}&interval={}&limit={}", self.base_url, symbol, interval, limit);
        let resp = self.http.get(&url).send().await?.json().await?;
        Ok(resp)
    }

    pub async fn ticker_24h(&self, symbol: &str) -> Result<Ticker24h, ExchangeError> {
        let url = format!("{}/api/v1/ticker/24hr?symbol={}", self.base_url, symbol);
        let resp = self.http.get(&url).send().await?.json().await?;
        Ok(resp)
    }

    pub async fn recent_trades(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<Trade>, ExchangeError> {
        let limit = limit.unwrap_or(50);
        let url = format!("{}/api/v1/trades?symbol={}&limit={}", self.base_url, symbol, limit);
        let resp = self.http.get(&url).send().await?.json().await?;
        Ok(resp)
    }

    // ── Authenticated Endpoints ───────────────────────────────────────────

    pub async fn place_order(&self, request: PlaceOrderRequest) -> Result<PlaceOrderResponse, ExchangeError> {
        let path = "/api/v1/orders";
        let headers = self.auth_headers("POST", path);
        let mut req = self.http.post(format!("{}{}", self.base_url, path))
            .json(&request);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ExchangeError::Api { status, message: msg });
        }
        Ok(resp.json().await?)
    }

    pub async fn cancel_order(&self, order_id: &str) -> Result<serde_json::Value, ExchangeError> {
        let path = format!("/api/v1/orders/{}", order_id);
        let headers = self.auth_headers("DELETE", &path);
        let mut req = self.http.delete(format!("{}{}", self.base_url, path));
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ExchangeError::Api { status, message: msg });
        }
        Ok(resp.json().await?)
    }

    pub async fn get_open_orders(&self) -> Result<Vec<Order>, ExchangeError> {
        let path = format!("/api/v1/orders/open/{}", self.user_id);
        let headers = self.auth_headers("GET", &path);
        let mut req = self.http.get(format!("{}{}", self.base_url, path));
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await?.json().await?;
        Ok(resp)
    }

    pub async fn get_portfolio(&self) -> Result<Portfolio, ExchangeError> {
        let path = format!("/api/v1/portfolio/{}", self.user_id);
        let headers = self.auth_headers("GET", &path);
        let mut req = self.http.get(format!("{}{}", self.base_url, path));
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await?.json().await?;
        Ok(resp)
    }

    pub async fn get_positions(&self) -> Result<Vec<Position>, ExchangeError> {
        let path = format!("/api/v1/futures/positions/{}", self.user_id);
        let headers = self.auth_headers("GET", &path);
        let mut req = self.http.get(format!("{}{}", self.base_url, path));
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await?.json().await?;
        Ok(resp)
    }

    pub async fn deposit(&self, asset: &str, amount: f64) -> Result<Balance, ExchangeError> {
        let path = "/api/v1/deposit";
        let headers = self.auth_headers("POST", path);
        let body = serde_json::json!({
            "user_id": self.user_id,
            "asset": asset,
            "amount": amount,
        });
        let mut req = self.http.post(format!("{}{}", self.base_url, path))
            .json(&body);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await?.json().await?;
        Ok(resp)
    }

    // ── WebSocket Streaming ───────────────────────────────────────────────

    pub async fn connect_ws(
        &self,
        symbols: &[String],
    ) -> Result<(
        futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::Message,
        >,
        futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    ), ExchangeError> {
        let ws_url = self.base_url.replace("http://", "ws://").replace("https://", "wss://");
        let symbol_filter = symbols.join(",");
        let url = if symbol_filter.is_empty() {
            format!("{}/api/v1/ws", ws_url)
        } else {
            format!("{}/api/v1/ws?symbols={}", ws_url, symbol_filter)
        };

        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| ExchangeError::WebSocket(e.to_string()))?;

        Ok(ws_stream.split())
    }

    // ── Convenience Methods ───────────────────────────────────────────────

    /// Place a market buy order
    pub async fn market_buy(&self, symbol: &str, quantity: f64) -> Result<PlaceOrderResponse, ExchangeError> {
        self.place_order(PlaceOrderRequest {
            user_id: self.user_id.clone(),
            symbol: symbol.to_string(),
            side: "Buy".to_string(),
            order_type: "Market".to_string(),
            price: None,
            quantity,
            time_in_force: None,
            trigger_price: None,
        }).await
    }

    /// Place a market sell order
    pub async fn market_sell(&self, symbol: &str, quantity: f64) -> Result<PlaceOrderResponse, ExchangeError> {
        self.place_order(PlaceOrderRequest {
            user_id: self.user_id.clone(),
            symbol: symbol.to_string(),
            side: "Sell".to_string(),
            order_type: "Market".to_string(),
            price: None,
            quantity,
            time_in_force: None,
            trigger_price: None,
        }).await
    }

    /// Place a limit buy order
    pub async fn limit_buy(&self, symbol: &str, price: f64, quantity: f64) -> Result<PlaceOrderResponse, ExchangeError> {
        self.place_order(PlaceOrderRequest {
            user_id: self.user_id.clone(),
            symbol: symbol.to_string(),
            side: "Buy".to_string(),
            order_type: "Limit".to_string(),
            price: Some(price),
            quantity,
            time_in_force: Some("Gtc".to_string()),
            trigger_price: None,
        }).await
    }

    /// Place a limit sell order
    pub async fn limit_sell(&self, symbol: &str, price: f64, quantity: f64) -> Result<PlaceOrderResponse, ExchangeError> {
        self.place_order(PlaceOrderRequest {
            user_id: self.user_id.clone(),
            symbol: symbol.to_string(),
            side: "Sell".to_string(),
            order_type: "Limit".to_string(),
            price: Some(price),
            quantity,
            time_in_force: Some("Gtc".to_string()),
            trigger_price: None,
        }).await
    }

    /// Get best bid and ask from orderbook
    pub async fn best_bid_ask(&self, symbol: &str) -> Result<(f64, f64), ExchangeError> {
        let ob = self.orderbook(symbol, Some(1)).await?;
        let best_bid = ob.bids.first().map(|l| l.price).unwrap_or(0.0);
        let best_ask = ob.asks.first().map(|l| l.price).unwrap_or(f64::MAX);
        Ok((best_bid, best_ask))
    }

    /// Get mid price
    pub async fn mid_price(&self, symbol: &str) -> Result<f64, ExchangeError> {
        let (bid, ask) = self.best_bid_ask(symbol).await?;
        Ok((bid + ask) / 2.0)
    }
}
