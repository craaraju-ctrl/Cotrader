//! CoTrader Broker — Tredo Exchange adapter
//!
//! Calls Tredo Exchange REST API with HMAC-SHA256 authentication.
//! Maps CoTrader's BrokerAdapter trait to Tredo's order/balance endpoints.
//!
//! ## Position Sync Architecture
//!
//! ```text
//! ExecutionCoordinator → TredoBroker.place_order() → Tredo REST API
//!                                                         ↓
//!                    TredoBroker.sync_position() ← Tredo order confirmation
//!                                                         ↓
//!                    TredoBroker.close_position() → Tredo REST API
//! ```
//!
//! By default (when COTRADER_BASE_URL is set), ALL trades — paper and live —
//! are mirrored to Tredo Exchange so the Tredo UI always reflects the current
//! portfolio state. This is the "always-sync" guarantee.

use async_trait::async_trait;
use cotrader_core::paper_engine::{
    BrokerAdapter, CloseReason, ClosedTrade, OrderRequest, OrderStatus, PortfolioSummary,
    Position, RiskCheckResult, TradingMode,
};
use cotrader_core::TradeDirection;

/// Map CoTrader symbols to Tredo Exchange paired symbols.
/// CoTrader uses bare symbols: "BTC", "ETH", "SOL"
/// Tredo expects paired symbols: "BTC/USD", "ETH/USD", "SOL/USD"
fn to_tredo_symbol(symbol: &str) -> String {
    if symbol.contains('/') {
        symbol.to_string()
    } else {
        format!("{}/USD", symbol)
    }
}

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use tokio::sync::RwLock;

// ── Backward Compatibility ────────────────────────────────────────────────
// The struct was renamed from `CoTraderBroker` → `TredoBroker`. We keep
// `CoTraderBroker` as a type alias so existing integration tests and users
// of the old name continue to compile.
#[doc(hidden)]
pub type CoTraderBroker = TredoBroker;

type HmacSha256 = Hmac<Sha256>;

/// Complete position record tracked locally for reconciliation with Tredo.
#[derive(Debug, Clone)]
pub struct LocalPositionRecord {
    pub symbol: String,
    pub direction: TradeDirection,
    pub qty: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub unrealized_pnl: f64,
    pub opened_at: chrono::DateTime<chrono::Utc>,
    pub tredo_order_id: Option<String>,
}

/// Tredo Exchange broker adapter — signs requests with HMAC-SHA256 and
/// calls Tredo's REST API for order placement, cancellation, and queries.
///
/// ## Default Sync Behavior
/// When `COTRADER_BASE_URL` is set, ALL trades (paper + live) are mirrored
/// to the Tredo Exchange. This ensures the Tredo UI always reflects the
/// complete portfolio state, even when running in paper mode.
pub struct TredoBroker {
    base_url: String,
    api_key: String,
    secret_key: String,
    user_id: String,
    client: reqwest::Client,
    /// Local position cache — tracks positions that have been synced to Tredo
    /// so we can reconcile and provide real position data (with entry prices, P&L).
    local_positions: Arc<RwLock<Vec<LocalPositionRecord>>>,
}

impl TredoBroker {
    /// Create a new Tredo broker with explicit parameters (backward-compatible constructor).
    ///
    /// This matches the old `CoTraderBroker::new(base_url, api_key, secret_key, user_id)`
    /// signature used by integration tests.
    pub fn new(base_url: &str, api_key: &str, secret_key: &str, user_id: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            secret_key: secret_key.to_string(),
            user_id: user_id.to_string(),
            client: reqwest::Client::new(),
            local_positions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create a new Tredo broker from environment variables.
    ///
    /// Expected env vars:
    /// - `COTRADER_BASE_URL` — Tredo exchange URL (e.g. http://localhost:8080)
    /// - `COTRADER_API_KEY` — API key (trd_...)
    /// - `COTRADER_SECRET_KEY` — HMAC secret
    /// - `COTRADER_USER_ID` — User ID on Tredo
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("COTRADER_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());
        let api_key =
            std::env::var("COTRADER_API_KEY").unwrap_or_default();
        let secret_key =
            std::env::var("COTRADER_SECRET_KEY").unwrap_or_default();
        let user_id =
            std::env::var("COTRADER_USER_ID").unwrap_or_else(|_| "orchestra".into());

        tracing::info!(
            url = %base_url,
            user = %user_id,
            has_key = !api_key.is_empty(),
            "TredoBroker initialized — ALL trades will sync to Tredo Exchange by default"
        );

        Self {
            base_url,
            api_key,
            secret_key,
            user_id,
            client: reqwest::Client::new(),
            local_positions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Sign a request: HMAC-SHA256(secret, METHOD + PATH + NONCE)
    /// Tredo uses the raw UTF-8 bytes of the secret key string (not hex-decoded).
    fn sign(&self, method: &str, path: &str, nonce: &str) -> String {
        let message = format!("{}{}{}", method, path, nonce);
        let mut mac =
            HmacSha256::new_from_slice(self.secret_key.as_bytes()).expect("HMAC key");
        mac.update(message.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// Build authenticated headers for a Tredo API call.
    fn auth_headers(&self, method: &str, path: &str) -> Vec<(&str, String)> {
        let nonce = chrono::Utc::now().timestamp_millis().to_string();
        let signature = self.sign(method, path, &nonce);
        vec![
            ("X-API-Key", self.api_key.clone()),
            ("X-Signature", signature),
            ("X-Nonce", nonce),
        ]
    }

    /// Place an order on Tredo Exchange.
    async fn tredo_place_order(
        &self,
        symbol: &str,
        side: &str,
        order_type: &str,
        quantity: f64,
        price: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<serde_json::Value, String> {
        let path = "/api/v1/orders";
        let url = format!("{}{}", self.base_url, path);

        let mut body = serde_json::json!({
            "user_id": self.user_id,
            "symbol": symbol,
            "side": side,
            "type": order_type,
            "quantity": quantity,
        });

        if let Some(p) = price {
            body["price"] = serde_json::json!(p);
        }
        if let Some(sl) = stop_loss {
            body["stop_loss"] = serde_json::json!(sl);
        }
        if let Some(tp) = take_profit {
            body["take_profit"] = serde_json::json!(tp);
        }

        let headers = self.auth_headers("POST", path);
        let mut req = self.client.post(&url).json(&body);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            tracing::warn!(
                status = %status,
                body = %text,
                symbol = %symbol,
                "Tredo place_order failed"
            );
            return Err(format!("Tredo API error ({}): {}", status, text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Get all balances from Tredo.
    async fn tredo_get_balances(&self) -> Result<Vec<serde_json::Value>, String> {
        let path = format!("/api/v1/balances/{}", self.user_id);
        let url = format!("{}{}", self.base_url, &path);

        let headers = self.auth_headers("GET", &path);
        let mut req = self.client.get(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        let text = resp.text().await.unwrap_or_default();

        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

        // Extract the "balances" array from the response
        parsed
            .get("balances")
            .and_then(|v| v.as_array())
            .cloned()
            .ok_or_else(|| format!("Unexpected balance response: {}", text))
    }

    /// Get positions from Tredo via the futures positions endpoint.
    async fn tredo_get_futures_positions(&self) -> Result<Vec<serde_json::Value>, String> {
        let path = format!("/api/v1/futures/positions/{}", self.user_id);
        let url = format!("{}{}", self.base_url, &path);

        let headers = self.auth_headers("GET", &path);
        let mut req = self.client.get(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        let text = resp.text().await.unwrap_or_default();

        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

        // Extract positions from response: { "user_id": "...", "positions": [...] }
        // If the positions key is missing, return an empty array (not an error)
        match parsed.get("positions").and_then(|v| v.as_array()) {
            Some(arr) => Ok(arr.clone()),
            None => {
                tracing::warn!(response = %text, "Tredo positions response missing 'positions' array");
                Ok(Vec::new())
            }
        }
    }

    /// Get open orders from Tredo.
    #[allow(dead_code)]
    async fn tredo_get_open_orders(&self) -> Result<Vec<serde_json::Value>, String> {
        let path = format!("/api/v1/orders/open/{}", self.user_id);
        let url = format!("{}{}", self.base_url, &path);

        let headers = self.auth_headers("GET", &path);
        let mut req = self.client.get(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        let text = resp.text().await.unwrap_or_default();

        serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Cancel an order on Tredo.
    async fn tredo_cancel_order(&self, order_id: &str) -> Result<(), String> {
        let path = format!("/api/v1/orders/{}", order_id);
        let url = format!("{}{}", self.base_url, &path);

        let headers = self.auth_headers("DELETE", &path);
        let mut req = self.client.delete(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(format!("Cancel failed ({}): {}", status, text))
        }
    }

    /// Sync a local position to Tredo Exchange. Called after every trade execution
    /// (both paper and live) to ensure the Tredo UI always reflects current state.
    ///
    /// This is the core of the "always-sync" guarantee. It places an order on Tredo
    /// mirroring the local trade, then caches the position for future reconciliation.
    pub async fn sync_position(
        &self,
        symbol: &str,
        direction: TradeDirection,
        qty: f64,
        entry_price: f64,
        stop_loss: f64,
        take_profit: f64,
        _strategy: Option<String>,
    ) -> Result<String, String> {
        let side = match direction {
            TradeDirection::Long => "Buy",
            TradeDirection::Short => "Sell",
        };

        tracing::info!(
            symbol = %symbol,
            side = %side,
            qty = qty,
            entry = entry_price,
            sl = stop_loss,
            tp = take_profit,
            "TredoBroker syncing position"
        );

        // Place the order on Tredo Exchange
        let resp = self
            .tredo_place_order(symbol, side, "Market", qty, Some(entry_price), Some(stop_loss), Some(take_profit))
            .await?;

        let order_id = resp
            .get("order_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Cache the position locally for get_positions() to return real data
        {
            let mut positions = self.local_positions.write().await;
            // Remove any existing position for this symbol (replace)
            positions.retain(|p| p.symbol != symbol);
            positions.push(LocalPositionRecord {
                symbol: symbol.to_string(),
                direction,
                qty,
                entry_price,
                current_price: entry_price,
                stop_loss,
                take_profit,
                unrealized_pnl: 0.0,
                opened_at: chrono::Utc::now(),
                tredo_order_id: Some(order_id.clone()),
            });
        }

        tracing::info!(
            order_id = %order_id,
            symbol = %symbol,
            "TredoBroker position synced"
        );

        Ok(order_id)
    }

    /// Remove a synced position from local cache (called when position is closed).
    pub async fn remove_synced_position(&self, symbol: &str) {
        let mut positions = self.local_positions.write().await;
        positions.retain(|p| p.symbol != symbol);
        tracing::info!(symbol = %symbol, "TredoBroker removed synced position");
    }

    /// Update a cached position's current price and unrealized P&L.
    pub async fn update_cached_position(&self, symbol: &str, current_price: f64) {
        let mut positions = self.local_positions.write().await;
        if let Some(pos) = positions.iter_mut().find(|p| p.symbol == symbol) {
            pos.current_price = current_price;
            pos.unrealized_pnl = match pos.direction {
                TradeDirection::Long => (current_price - pos.entry_price) * pos.qty,
                TradeDirection::Short => (pos.entry_price - current_price) * pos.qty,
            };
        }
    }
}

#[async_trait]
impl BrokerAdapter for TredoBroker {
    async fn connect(&self) -> Result<(), String> {
        // Verify Tredo is reachable
        let url = format!("{}/api/v1/health", self.base_url);
        let resp = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| format!("Cannot reach Tredo at {}: {}", self.base_url, e))?;

        if resp.status().is_success() {
            tracing::info!(url = %self.base_url, "TredoBroker connected — full position sync active");
            Ok(())
        } else {
            Err(format!("Tredo health check failed: {}", resp.status()))
        }
    }

    async fn disconnect(&self) -> Result<(), String> {
        Ok(())
    }

    async fn place_order(
        &self,
        request: OrderRequest,
        _market_price: f64,
    ) -> Result<String, String> {
        let side = match request.direction {
            TradeDirection::Long => "Buy",
            TradeDirection::Short => "Sell",
        };

        let (order_type, price) = match request.order_type {
            cotrader_core::paper_engine::OrderType::Market => ("Market", None),
            cotrader_core::paper_engine::OrderType::Limit => ("Limit", request.price),
            cotrader_core::paper_engine::OrderType::StopLoss => ("StopLoss", request.stop_loss),
            cotrader_core::paper_engine::OrderType::StopLossLimit => {
                ("StopLimit", request.price)
            }
        };

        let quantity = request.qty;

        tracing::info!(
            symbol = %request.symbol,
            side = %side,
            type = %order_type,
            qty = quantity,
            price = ?price,
            "TredoBroker placing order"
        );

        let tredo_sym = to_tredo_symbol(&request.symbol);
        let resp = self
            .tredo_place_order(
                &tredo_sym,
                side,
                order_type,
                quantity,
                price,
                request.stop_loss,
                request.take_profit,
            )
            .await?;

        let order_id = resp
            .get("order_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Cache the position after successful placement
        {
            let mut positions = self.local_positions.write().await;
            positions.retain(|p| p.symbol != request.symbol);
            positions.push(LocalPositionRecord {
                symbol: request.symbol.clone(),
                direction: request.direction,
                qty: request.qty,
                entry_price: request.price.unwrap_or(_market_price),
                current_price: _market_price,
                stop_loss: request.stop_loss.unwrap_or(0.0),
                take_profit: request.take_profit.unwrap_or(0.0),
                unrealized_pnl: 0.0,
                opened_at: chrono::Utc::now(),
                tredo_order_id: Some(order_id.clone()),
            });
        }

        tracing::info!(
            order_id = %order_id,
            status = ?resp.get("status"),
            filled = ?resp.get("filled_quantity"),
            "TredoBroker order placed"
        );

        Ok(order_id)
    }

    async fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        tracing::info!(order_id = %order_id, "TredoBroker cancelling order");
        self.tredo_cancel_order(order_id).await
    }

    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        // PRIMARY: Return locally-cached positions with real entry prices and P&L.
        // These are populated by place_order() and sync_position() calls.
        {
            let local = self.local_positions.read().await;
            if !local.is_empty() {
                let mut positions = Vec::with_capacity(local.len());
                for lp in local.iter() {
                    let dir_label = if lp.direction == TradeDirection::Long { "LONG" } else { "SHORT" };
                    positions.push(Position {
                        id: format!("tredo-{}-{}", lp.symbol, lp.opened_at.timestamp()),
                        symbol: lp.symbol.clone(),
                        direction: lp.direction,
                        qty: lp.qty,
                        entry_price: lp.entry_price,
                        current_price: lp.current_price,
                        stop_loss: lp.stop_loss,
                        take_profit: lp.take_profit,
                        unrealized_pnl: lp.unrealized_pnl,
                        unrealized_pnl_pct: if lp.entry_price > 0.0 {
                            (lp.unrealized_pnl / (lp.entry_price * lp.qty)) * 100.0
                        } else {
                            0.0
                        },
                        status: cotrader_core::paper_engine::PositionStatus::Open,
                        opened_at: lp.opened_at,
                        closed_at: None,
                        strategy: Some(format!("rat-{}", dir_label)),
                        order_id: lp.tredo_order_id.clone().unwrap_or_default(),
                    });
                }
                tracing::info!(count = positions.len(), "TredoBroker returning cached positions with real data");
                return Ok(positions);
            }
        }

        // FALLBACK: Try Tredo's futures positions endpoint (for externally-opened positions)
        let tredo_positions = match self.tredo_get_futures_positions().await {
            Ok(positions) => positions,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch Tredo futures positions, falling back to balances");
                Vec::new()
            }
        };

        if !tredo_positions.is_empty() {
            let mut positions = Vec::new();
            for p in &tredo_positions {
                let symbol = p.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let side = p.get("side").and_then(|v| v.as_str()).unwrap_or("long");
                let direction = if side == "long" || side == "LONG" {
                    TradeDirection::Long
                } else {
                    TradeDirection::Short
                };
                let qty = p.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let entry = p.get("entry_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let current = p.get("current_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let upnl = p.get("unrealized_pnl").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let sl = p.get("stop_loss").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let tp = p.get("take_profit").and_then(|v| v.as_f64()).unwrap_or(0.0);

                if !symbol.is_empty() && qty > 0.0 {
                    positions.push(Position {
                        id: format!("tredo-fut-{}", symbol),
                        symbol,
                        direction,
                        qty,
                        entry_price: entry,
                        current_price: current,
                        stop_loss: sl,
                        take_profit: tp,
                        unrealized_pnl: upnl,
                        unrealized_pnl_pct: if entry > 0.0 { (upnl / (entry * qty)) * 100.0 } else { 0.0 },
                        status: cotrader_core::paper_engine::PositionStatus::Open,
                        opened_at: chrono::Utc::now(),
                        closed_at: None,
                        strategy: Some("tredo-futures".to_string()),
                        order_id: String::new(),
                    });
                }
            }
            if !positions.is_empty() {
                tracing::info!(count = positions.len(), "TredoBroker returning positions from Tredo futures API");
                return Ok(positions);
            }
        }

        // LAST RESORT: Get balances from Tredo
        let balances = self.tredo_get_balances().await?;

        let mut positions = Vec::new();
        for b in &balances {
            let asset = b.get("asset").and_then(|v| v.as_str()).unwrap_or("");
            let total = b.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);

            if asset == "USD" || total <= 0.0 {
                continue;
            }

            positions.push(Position {
                id: format!("tredo-bal-{}", asset),
                symbol: format!("{}/USD", asset),
                direction: TradeDirection::Long,
                qty: total,
                entry_price: 0.0,
                current_price: 0.0,
                stop_loss: 0.0,
                take_profit: 0.0,
                unrealized_pnl: 0.0,
                unrealized_pnl_pct: 0.0,
                status: cotrader_core::paper_engine::PositionStatus::Open,
                opened_at: chrono::Utc::now(),
                closed_at: None,
                strategy: None,
                order_id: String::new(),
            });
        }

        Ok(positions)
    }

    async fn get_summary(&self) -> Result<PortfolioSummary, String> {
        // Compute real portfolio summary from cached positions
        {
            let local = self.local_positions.read().await;
            if !local.is_empty() {
                let total_value: f64 = local.iter().map(|p| p.current_price * p.qty).sum();
                let total_upnl: f64 = local.iter().map(|p| p.unrealized_pnl).sum();
                return Ok(PortfolioSummary {
                    cash: 0.0, // Tredo tracks USD balance separately
                    equity: total_value,
                    margin_used: 0.0,
                    free_margin: total_value,
                    daily_pnl: total_upnl,
                    daily_pnl_pct: if total_value > 0.0 { (total_upnl / total_value) * 100.0 } else { 0.0 },
                    total_trades: local.len() as u32,
                    winning_trades: local.iter().filter(|p| p.unrealized_pnl > 0.0).count() as u32,
                    losing_trades: local.iter().filter(|p| p.unrealized_pnl < 0.0).count() as u32,
                    win_rate: 0.0,
                    consecutive_losses: 0,
                    max_drawdown: 0.0,
                    max_drawdown_pct: 0.0,
                    open_positions: local.len(),
                    total_pnl_all_time: total_upnl,
                });
            }
        }

        // Fallback: Tredo balance-based summary
        let balances = self.tredo_get_balances().await?;
        let mut total_usd = 0.0;
        for b in &balances {
            let asset = b.get("asset").and_then(|v| v.as_str()).unwrap_or("");
            let total = b.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if asset == "USD" {
                total_usd += total;
            }
        }

        Ok(PortfolioSummary {
            cash: total_usd,
            equity: total_usd,
            margin_used: 0.0,
            free_margin: total_usd,
            daily_pnl: 0.0,
            daily_pnl_pct: 0.0,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            win_rate: 0.0,
            consecutive_losses: 0,
            max_drawdown: 0.0,
            max_drawdown_pct: 0.0,
            open_positions: 0,
            total_pnl_all_time: 0.0,
        })
    }

    async fn get_order_status(&self, order_id: &str) -> Result<OrderStatus, String> {
        let path = format!("/api/v1/orders/{}", order_id);
        let url = format!("{}{}", self.base_url, &path);

        let headers = self.auth_headers("GET", &path);
        let mut req = self.client.get(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        let text = resp.text().await.unwrap_or_default();

        let order: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("JSON parse: {}", e))?;

        let status_str = order
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");

        match status_str {
            "Filled" => Ok(OrderStatus::Filled),
            "PartiallyFilled" => {
                let filled = order
                    .get("filled_quantity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                Ok(OrderStatus::PartiallyFilled { filled_qty: filled })
            }
            "Cancelled" => Ok(OrderStatus::Cancelled),
            "Rejected" => Ok(OrderStatus::Rejected {
                reason: "Rejected by exchange".into(),
            }),
            "Expired" => Ok(OrderStatus::Expired),
            _ => Ok(OrderStatus::Pending),
        }
    }

    async fn get_recent_trades(&self, _limit: usize) -> Result<Vec<ClosedTrade>, String> {
        Ok(vec![])
    }

    async fn update_price(
        &self,
        symbol: &str,
        market_price: f64,
    ) -> Result<Vec<ClosedTrade>, String> {
        self.update_cached_position(symbol, market_price).await;
        Ok(vec![])
    }

    async fn close_position(
        &self,
        position_id: &str,
        exit_price: f64,
    ) -> Result<ClosedTrade, String> {
        // Parse position_id to extract symbol
        let symbol = position_id
            .strip_prefix("tredo-")
            .map(|s| {
                // Handle "tredo-BTC-1234567890" → "BTC"
                s.rsplit_once('-').map(|(sym, _)| sym).unwrap_or(s)
            })
            .unwrap_or(position_id);

        // Get position details from local cache before removing
        let (direction, qty, entry_price) = {
            let positions = self.local_positions.read().await;
            positions
                .iter()
                .find(|p| p.symbol == symbol)
                .map(|p| (p.direction, p.qty, p.entry_price))
                .unwrap_or((TradeDirection::Long, 1.0, 0.0))
        };

        let close_side = match direction {
            TradeDirection::Long => "Sell",
            TradeDirection::Short => "Buy",
        };

        // Place closing order on Tredo
        let resp = self
            .tredo_place_order(symbol, close_side, "Market", qty, None, None, None)
            .await
            .map_err(|e| format!("Failed to close position: {}", e))?;

        let order_id = resp
            .get("order_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Remove from local cache
        self.remove_synced_position(symbol).await;

        let realized_pnl = match direction {
            TradeDirection::Long => (exit_price - entry_price) * qty,
            TradeDirection::Short => (entry_price - exit_price) * qty,
        };

        Ok(ClosedTrade {
            id: format!("close-{}-{}", symbol, chrono::Utc::now().timestamp_millis()),
            symbol: symbol.to_string(),
            direction,
            qty,
            entry_price,
            exit_price,
            realized_pnl,
            realized_pnl_pct: if entry_price > 0.0 { (realized_pnl / (entry_price * qty)) * 100.0 } else { 0.0 },
            close_reason: CloseReason::Manual,
            opened_at: chrono::Utc::now(),
            closed_at: chrono::Utc::now(),
            duration_secs: 0,
            strategy: Some("rat-tredo".to_string()),
            order_id,
        })
    }

    async fn check_risk(
        &self,
        _symbol: &str,
        _estimated_cost: f64,
    ) -> Result<RiskCheckResult, String> {
        Ok(RiskCheckResult {
            passed: true,
            max_position_size_ok: true,
            daily_loss_limit_ok: true,
            drawdown_ok: true,
            concentration_ok: true,
            portfolio_heat_ok: true,
            warnings: vec![],
        })
    }

    async fn reset(&self) -> Result<(), String> {
        let mut positions = self.local_positions.write().await;
        positions.clear();
        Ok(())
    }

    fn mode(&self) -> TradingMode {
        TradingMode::Live
    }

    fn broker_name(&self) -> &str {
        "Tredo"
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a TredoBroker with dummy credentials for testing.
    fn test_broker() -> TredoBroker {
        TredoBroker::new("http://localhost:9999", "test_key", "test_secret", "test_user")
    }

    /// Helper to seed a position into the broker's local cache for testing.
    async fn seed_position(broker: &TredoBroker, symbol: &str, direction: TradeDirection, qty: f64, entry: f64) {
        let mut positions = broker.local_positions.write().await;
        positions.push(LocalPositionRecord {
            symbol: symbol.to_string(),
            direction,
            qty,
            entry_price: entry,
            current_price: entry,
            stop_loss: entry * 0.95,
            take_profit: entry * 1.05,
            unrealized_pnl: 0.0,
            opened_at: chrono::Utc::now(),
            tredo_order_id: Some("test-order-id".to_string()),
        });
    }

    /// Helper to count positions in the cache.
    async fn cache_count(broker: &TredoBroker) -> usize {
        broker.local_positions.read().await.len()
    }

    // ── new() ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_new_broker_has_empty_cache() {
        let broker = test_broker();
        assert_eq!(cache_count(&broker).await, 0, "New broker should have empty cache");

        // NOTE: Cannot call get_positions() on empty cache — it makes HTTP calls
        // as part of the 3-tier fallback (cache → futures API → balances).
        // Direct cache access is used here since we only need to verify emptiness.
        let positions = broker.local_positions.read().await;
        assert!(positions.is_empty(), "Cache should be empty for new broker");
    }

    #[tokio::test]
    async fn test_new_broker_has_correct_defaults() {
        let broker = test_broker();
        assert_eq!(broker.broker_name(), "Tredo");
        assert_eq!(broker.mode(), TradingMode::Live);
        assert_eq!(broker.base_url, "http://localhost:9999");
        assert_eq!(broker.user_id, "test_user");
    }

    // ── remove_synced_position() ────────────────────────────────────────────

    #[tokio::test]
    async fn test_remove_nonexistent_symbol_is_noop() {
        let broker = test_broker();
        // Remove symbol that doesn't exist — should not panic
        broker.remove_synced_position("NONEXISTENT").await;
        assert_eq!(cache_count(&broker).await, 0);
    }

    #[tokio::test]
    async fn test_remove_existing_symbol() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;
        seed_position(&broker, "ETH", TradeDirection::Long, 10.0, 3000.0).await;
        assert_eq!(cache_count(&broker).await, 2);

        broker.remove_synced_position("BTC").await;
        assert_eq!(cache_count(&broker).await, 1);

        // ETH should still be there
        let remaining = broker.local_positions.read().await;
        assert_eq!(remaining[0].symbol, "ETH");
    }

    #[tokio::test]
    async fn test_remove_all_positions_one_by_one() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;
        seed_position(&broker, "ETH", TradeDirection::Long, 10.0, 3000.0).await;
        seed_position(&broker, "SOL", TradeDirection::Short, 5.0, 150.0).await;

        broker.remove_synced_position("BTC").await;
        broker.remove_synced_position("ETH").await;
        broker.remove_synced_position("SOL").await;

        assert_eq!(cache_count(&broker).await, 0);
    }

    // ── update_cached_position() ───────────────────────────────────────────

    #[tokio::test]
    async fn test_update_nonexistent_symbol_is_noop() {
        let broker = test_broker();
        // Should not panic
        broker.update_cached_position("NONEXISTENT", 100.0).await;
        assert_eq!(cache_count(&broker).await, 0);
    }

    #[tokio::test]
    async fn test_update_long_position_pnl() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;

        // Price goes up to 51000 — Long should have positive P&L
        broker.update_cached_position("BTC", 51000.0).await;

        let pos = broker.local_positions.read().await;
        let btc = &pos[0];
        assert!((btc.current_price - 51000.0).abs() < 1e-9, "Price should be updated");
        assert!((btc.unrealized_pnl - 1000.0).abs() < 1e-6, "Long P&L should be +1000 (price up)");

        // Price goes down to 49000 — Long should have negative P&L
        broker.update_cached_position("BTC", 49000.0).await;

        let pos = broker.local_positions.read().await;
        let btc = &pos[0];
        assert!((btc.unrealized_pnl - (-1000.0)).abs() < 1e-6, "Long P&L should be -1000 (price down)");
    }

    #[tokio::test]
    async fn test_update_short_position_pnl() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Short, 2.0, 50000.0).await;

        // Price goes UP to 52000 — Short should have negative P&L (+2 BTC * -2000)
        broker.update_cached_position("BTC", 52000.0).await;

        let pos = broker.local_positions.read().await;
        let btc = &pos[0];
        assert!((btc.current_price - 52000.0).abs() < 1e-9);
        assert!((btc.unrealized_pnl - (-4000.0)).abs() < 1e-6,
            "Short P&L should be -4000 (price up 2000 * 2 BTC), got {}", btc.unrealized_pnl);

        // Price goes DOWN to 48000 — Short should have positive P&L
        broker.update_cached_position("BTC", 48000.0).await;

        let pos = broker.local_positions.read().await;
        let btc = &pos[0];
        assert!((btc.unrealized_pnl - 4000.0).abs() < 1e-6,
            "Short P&L should be +4000 (price down 2000 * 2 BTC), got {}", btc.unrealized_pnl);
    }

    #[tokio::test]
    async fn test_update_multiple_symbols_independently() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;
        seed_position(&broker, "ETH", TradeDirection::Short, 10.0, 3000.0).await;

        // Only update BTC
        broker.update_cached_position("BTC", 51000.0).await;

        let pos = broker.local_positions.read().await;
        let btc = pos.iter().find(|p| p.symbol == "BTC").unwrap();
        assert!((btc.unrealized_pnl - 1000.0).abs() < 1e-6, "BTC P&L should be +1000");

        let eth = pos.iter().find(|p| p.symbol == "ETH").unwrap();
        assert!((eth.current_price - 3000.0).abs() < 1e-9, "ETH price should remain unchanged");
        assert!((eth.unrealized_pnl - 0.0).abs() < 1e-9, "ETH P&L should remain 0");
    }

    // ── get_positions() ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_positions_returns_cached_data() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.5, 50000.0).await;

        let positions = broker.get_positions().await.unwrap();
        assert_eq!(positions.len(), 1);

        let btc = &positions[0];
        assert_eq!(btc.symbol, "BTC");
        assert_eq!(btc.direction, TradeDirection::Long);
        assert!((btc.qty - 1.5).abs() < 1e-9, "Qty should be preserved as f64");
        assert!((btc.entry_price - 50000.0).abs() < 1e-9);
        assert!((btc.current_price - 50000.0).abs() < 1e-9);
        assert!((btc.stop_loss - 47500.0).abs() < 1e-9);
        assert!((btc.take_profit - 52500.0).abs() < 1e-9);
        assert!((btc.unrealized_pnl_pct - 0.0).abs() < 1e-9);
        assert!(btc.id.starts_with("tredo-BTC-"));
        assert!(!btc.order_id.is_empty());
    }

    #[tokio::test]
    async fn test_get_positions_unrealized_pnl_pct_calculation() {
        let broker = test_broker();
        // Long position that has gone up 10%
        seed_position(&broker, "BTC", TradeDirection::Long, 2.0, 50000.0).await;

        // Update price to simulate 10% gain
        broker.update_cached_position("BTC", 55000.0).await;

        let positions = broker.get_positions().await.unwrap();
        let btc = &positions[0];

        // P&L = (55000 - 50000) * 2 = 10000
        assert!((btc.unrealized_pnl - 10000.0).abs() < 1e-6, "P&L should be 10000");
        // P&L% = 10000 / (50000 * 2) * 100 = 10%
        assert!((btc.unrealized_pnl_pct - 10.0).abs() < 1e-6, "P&L% should be 10%, got {}", btc.unrealized_pnl_pct);
    }

    #[tokio::test]
    async fn test_get_positions_zero_entry_price_no_panic() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 0.0).await;

        // Should not panic on division by zero
        let positions = broker.get_positions().await.unwrap();
        assert_eq!(positions.len(), 1);
        assert!((positions[0].unrealized_pnl_pct - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_get_positions_returns_multiple_symbols() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;
        seed_position(&broker, "ETH", TradeDirection::Short, 10.0, 3000.0).await;
        seed_position(&broker, "SOL", TradeDirection::Long, 100.0, 150.0).await;

        let positions = broker.get_positions().await.unwrap();
        assert_eq!(positions.len(), 3);

        let symbols: Vec<&str> = positions.iter().map(|p| p.symbol.as_str()).collect();
        assert!(symbols.contains(&"BTC"));
        assert!(symbols.contains(&"ETH"));
        assert!(symbols.contains(&"SOL"));
    }

    // ── reset() ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_reset_clears_cache() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;
        seed_position(&broker, "ETH", TradeDirection::Long, 10.0, 3000.0).await;
        assert_eq!(cache_count(&broker).await, 2);

        broker.reset().await.unwrap();
        assert_eq!(cache_count(&broker).await, 0);

        // NOTE: Cannot call get_positions() on empty cache — it makes HTTP calls.
        let positions = broker.local_positions.read().await;
        assert!(positions.is_empty(), "Cache should be empty after reset");
    }

    #[tokio::test]
    async fn test_reset_empty_cache_is_noop() {
        let broker = test_broker();
        assert_eq!(cache_count(&broker).await, 0);
        broker.reset().await.unwrap();
        assert_eq!(cache_count(&broker).await, 0);
    }

    // ── Symbol replacement (sync_position semantics) ────────────────────────

    #[tokio::test]
    async fn test_sync_replaces_existing_symbol() {
        // Verify that syncing the same symbol replaces (doesn't duplicate)
        let broker = test_broker();

        // Add BTC via seed
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;
        assert_eq!(cache_count(&broker).await, 1);

        // Simulate symbol replacement logic from sync_position:
        {
            let mut positions = broker.local_positions.write().await;
            positions.retain(|p| p.symbol != "BTC");
            positions.push(LocalPositionRecord {
                symbol: "BTC".to_string(),
                direction: TradeDirection::Short, // direction changed
                qty: 2.0,                         // qty changed
                entry_price: 51000.0,
                current_price: 51000.0,
                stop_loss: 52000.0,
                take_profit: 49000.0,
                unrealized_pnl: 0.0,
                opened_at: chrono::Utc::now(),
                tredo_order_id: Some("new-order".to_string()),
            });
        }

        assert_eq!(cache_count(&broker).await, 1, "Should still have 1 position (replaced, not duplicated)");

        let pos = broker.local_positions.read().await;
        assert_eq!(pos[0].direction, TradeDirection::Short);
        assert!((pos[0].qty - 2.0).abs() < 1e-9);
        assert!((pos[0].entry_price - 51000.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_sync_broker_has_independent_caches() {
        // Two brokers should have independent caches
        let broker1 = test_broker();
        let broker2 = TredoBroker::new("http://localhost:9998", "key2", "secret2", "user2");

        seed_position(&broker1, "BTC", TradeDirection::Long, 1.0, 50000.0).await;
        assert_eq!(cache_count(&broker1).await, 1);
        assert_eq!(cache_count(&broker2).await, 0, "Second broker cache should be empty");
    }

    // ── Fractional qty precision ────────────────────────────────────────────

    #[tokio::test]
    async fn test_fractional_qty_precision() {
        let broker = test_broker();
        // 0.062 BTC — critical crypto fractional amount
        seed_position(&broker, "BTC", TradeDirection::Long, 0.062, 65000.0).await;

        let positions = broker.get_positions().await.unwrap();
        assert!(
            (positions[0].qty - 0.062).abs() < 1e-10,
            "Fractional qty 0.062 must be preserved exactly, got {}",
            positions[0].qty
        );

        // Update price and verify P&L is calculated with fractional qty
        broker.update_cached_position("BTC", 66000.0).await;
        let positions = broker.get_positions().await.unwrap();
        // P&L = (66000 - 65000) * 0.062 = 62.0
        assert!(
            (positions[0].unrealized_pnl - 62.0).abs() < 1e-9,
            "P&L should be 62.0 (1000 gain * 0.062 BTC), got {}",
            positions[0].unrealized_pnl
        );
    }

    // ── close_position dependency (symbol parsing from ID) ──────────────────

    #[tokio::test]
    async fn test_close_position_uses_cached_data_for_pnl() {
        // This tests the P&L logic used in close_position without needing HTTP
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 2.0, 50000.0).await;

        // Simulate what close_position does:
        let (direction, qty, entry_price) = {
            let positions = broker.local_positions.read().await;
            positions
                .iter()
                .find(|p| p.symbol == "BTC")
                .map(|p| (p.direction, p.qty, p.entry_price))
                .unwrap()
        };

        let exit_price = 51000.0;
        let realized_pnl = match direction {
            TradeDirection::Long => (exit_price - entry_price) * qty,
            TradeDirection::Short => (entry_price - exit_price) * qty,
        };

        assert!((realized_pnl - 2000.0).abs() < 1e-9,
            "close_position P&L should be 2000 (1000 gain * 2 BTC), got {}", realized_pnl);
    }

    #[tokio::test]
    async fn test_position_id_format() {
        let broker = test_broker();
        seed_position(&broker, "BTC", TradeDirection::Long, 1.0, 50000.0).await;

        let positions = broker.get_positions().await.unwrap();
        let id = &positions[0].id;

        assert!(id.starts_with("tredo-BTC-"), "Position ID should start with 'tredo-{{symbol}}-', got {}", id);

        // close_position parses this format: strip_prefix("tredo-") then rsplit_once('-')
        let symbol_from_id = id
            .strip_prefix("tredo-")
            .and_then(|s| s.rsplit_once('-'))
            .map(|(sym, _)| sym)
            .unwrap_or("");
        assert_eq!(symbol_from_id, "BTC", "close_position should extract 'BTC' from ID");
    }

    // ── Sync position (ignored — requires network) ─────────────────────────

    #[tokio::test]
    #[ignore = "Requires running Tredo Exchange server"]
    async fn test_sync_position_requires_tredo() {
        let broker = test_broker();
        let result = broker
            .sync_position("BTC", TradeDirection::Long, 0.062, 65000.0, 64000.0, 68000.0, None)
            .await;
        assert!(result.is_ok(), "sync_position should succeed when Tredo is reachable");
    }

    #[tokio::test]
    #[ignore = "Requires running Tredo Exchange server"]
    async fn test_place_order_requires_tredo() {
        let broker = test_broker();
        let req = OrderRequest {
            symbol: "BTC".to_string(),
            direction: TradeDirection::Long,
            order_type: cotrader_core::paper_engine::OrderType::Market,
            qty: 0.062,
            price: None,
            stop_loss: Some(64000.0),
            take_profit: Some(68000.0),
            strategy: Some("test".to_string()),
            client_order_id: None,
        };
        let result = broker.place_order(req, 65000.0).await;
        assert!(result.is_ok(), "place_order should succeed when Tredo is reachable");
    }
}

