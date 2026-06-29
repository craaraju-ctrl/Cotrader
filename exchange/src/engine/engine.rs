use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use crate::storage::WalStore;
use crate::types::{
    err_internal, err_order_not_found,
    ExchangeInfo, ExchangeResult, Order, OrderBookSnapshot, PlaceOrderResponse, Candle,
    Ticker24hr, OcoRequest, OcoResponse, PositionSide,
};
use super::matching::MatchingEngine;
use super::risk::RiskEngine;
use super::candles::CandleStore;
use super::futures::FuturesEngine;

#[derive(Clone)]
pub struct ExchangeEngine {
    inner: Arc<RwLock<MatchingEngine>>,
    store: Option<WalStore>,
    risk: Arc<RiskEngine>,
    futures: Arc<FuturesEngine>,
    candles: Arc<tokio::sync::RwLock<CandleStore>>,
    /// In-memory trade history — always populated regardless of DB
    trades: Arc<tokio::sync::RwLock<Vec<crate::types::Trade>>>,
    /// In-memory order history (all orders: open, filled, cancelled) — always populated
    order_history: Arc<tokio::sync::RwLock<Vec<crate::types::Order>>>,
}

impl ExchangeEngine {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MatchingEngine::new())),
            store: None,
            risk: Arc::new(RiskEngine::new()),
            futures: {
                let f = FuturesEngine::new();
                for mkt in crate::types::default_markets() {
                    f.init_funding_rate(&mkt.symbol);
                }
                Arc::new(f)
            },
            candles: Arc::new(tokio::sync::RwLock::new(CandleStore::new())),
            trades: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            order_history: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    pub async fn new_with_persistence(url: &str) -> ExchangeResult<Self> {
        let store = WalStore::new(url).await?;
        let (active_orders, trades) = store.recover_state().await?;
        let risk = Arc::new(RiskEngine::new());
        let mut engine = MatchingEngine::new();
        let candles = Arc::new(tokio::sync::RwLock::new(CandleStore::new()));
        // Rebuild candles from historical trades
        {
            let mut c = candles.write().await;
            for trade in &trades {
                c.add_trade(trade);
            }
        }
        for order in &active_orders {
            if order.is_active() {
                if order.is_stop_order() {
                    engine.add_trigger_order(order.clone());
                } else {
                    engine.get_or_create_book(&order.symbol).add_order(order.clone());
                }
                risk.recover_open_order(order);
            }
        }
        tracing::info!("Recovered {} orders, {} trades for candles", active_orders.len(), trades.len());
        Ok(Self {
            inner: Arc::new(RwLock::new(engine)),
            store: Some(store),
            risk,
            futures: {
                let f = FuturesEngine::new();
                for mkt in crate::types::default_markets() {
                    f.init_funding_rate(&mkt.symbol);
                }
                Arc::new(f)
            },
            candles,
            trades: Arc::new(tokio::sync::RwLock::new(trades)),
            order_history: Arc::new(tokio::sync::RwLock::new(active_orders)),
        })
    }

    pub fn risk_engine(&self) -> &Arc<RiskEngine> { &self.risk }
    pub fn futures_engine(&self) -> &Arc<FuturesEngine> { &self.futures }

    pub async fn place_order(&self, order: Order) -> ExchangeResult<PlaceOrderResponse> {
        // Get best ask for market order pricing
        let best_ask = if order.order_type == crate::types::OrderType::Market && order.side == crate::types::Side::Buy {
            let (_, ask) = self.get_best_bid_ask(&order.symbol).await;
            ask
        } else { None };

        // Get user's leverage for margin checking (1 = spot, >1 = futures margin)
        let lev = self.futures.get_leverage(&order.user_id, &order.symbol);

        self.risk.check_order(&order, best_ask, lev).await?;
        // Only lock funds for non-stop orders. Stop orders lock when triggered.
        if !order.is_stop_order() {
            self.risk.lock_for_order(&order, best_ask, lev).await?;
        }
        if let Some(ref s) = self.store { s.write_order(&order).await?; }
        // Copy fields before order is moved into process_order
        // (order history recorded AFTER process_order to capture final status)
        let order_type = order.order_type;
        let order_side = order.side;
        let order_user = order.user_id.clone();
        let order_symbol = order.symbol.clone();
        let mut eng = self.inner.write().await;
        let result = eng.process_order(order)?;
        for t in &result.trades {
            self.risk.settle_trade(&t.buyer_id, &t.seller_id, &t.symbol, t.price, t.quantity, t.taker_side).await;
        }

        // ── Futures position tracking after trades ──
        for t in &result.trades {
            self.futures.update_mark_price(&t.symbol, t.price);
            self.futures.open_position(&t.buyer_id, &t.symbol, crate::types::Side::Buy, t.price, t.quantity);
            self.futures.open_position(&t.seller_id, &t.symbol, crate::types::Side::Sell, t.price, t.quantity);
        }

        // Feed trades into in-memory trade store and candle store
        if !result.trades.is_empty() {
            {
                let mut t = self.trades.write().await;
                t.extend(result.trades.iter().cloned());
            }
            let mut c = self.candles.write().await;
            for t in &result.trades {
                c.add_trade(t);
            }
        }

        // Release excess locked funds for filled market orders
        // Market orders lock an estimated amount which may exceed the actual traded amount
        if order_type == crate::types::OrderType::Market && !result.trades.is_empty() {
            let (base, quote) = if let Some(idx) = order_symbol.find('/') {
                (order_symbol[..idx].to_string(), order_symbol[idx+1..].to_string())
            } else { (String::new(), String::new()) };
            match order_side {
                crate::types::Side::Buy => {
                    let bal = self.risk.get_balance(&order_user, &quote);
                    if bal.locked > 0.0 {
                        self.risk.unlock_locked(&order_user, &quote, bal.locked);
                    }
                }
                crate::types::Side::Sell => {
                    let bal = self.risk.get_balance(&order_user, &base);
                    if bal.locked > 0.0 {
                        self.risk.unlock_locked(&order_user, &base, bal.locked);
                    }
                }
            }
        }

        // Check for triggered stop orders after trades
        if !result.trades.is_empty() {
            let last_price = result.trades.last().unwrap().price;
            let traded_symbol = &result.trades[0].symbol;
            let triggered = eng.check_trigger_orders(traded_symbol, last_price);
            for stop_order in triggered {
                // Process each triggered stop order immediately
                // Get leverage for triggered stop order
                let stop_lev = self.futures.get_leverage(&stop_order.user_id, &stop_order.symbol);
                if let Err(e) = self.risk.check_order(&stop_order, None, stop_lev).await {
                    tracing::warn!("Triggered stop order rejected by risk: {}", e);
                    continue;
                }
                if let Err(e) = self.risk.lock_for_order(&stop_order, None, stop_lev).await {
                    tracing::warn!("Triggered stop order lock failed: {}", e);
                    continue;
                }
                let stop_symbol = stop_order.symbol.clone();
                match eng.process_order(stop_order) {
                    Ok(stop_resp) => {
                        for t in &stop_resp.trades {
                            self.risk.settle_trade(&t.buyer_id, &t.seller_id, &t.symbol, t.price, t.quantity, t.taker_side).await;
                        }
                        // ── Futures position tracking for triggered trades ──
                        for t in &stop_resp.trades {
                            self.futures.update_mark_price(&t.symbol, t.price);
                            self.futures.open_position(&t.buyer_id, &t.symbol, crate::types::Side::Buy, t.price, t.quantity);
                            self.futures.open_position(&t.seller_id, &t.symbol, crate::types::Side::Sell, t.price, t.quantity);
                        }
                        // Feed triggered trades into in-memory store and candle store
                        {
                            let mut mem_trades = self.trades.write().await;
                            mem_trades.extend(stop_resp.trades.iter().cloned());
                            let mut c = self.candles.write().await;
                            for t in &stop_resp.trades {
                                c.add_trade(t);
                            }
                        }
                        if let Some(ref s) = self.store {
                            // Write triggered trades to DB
                            for t in &stop_resp.trades {
                                if let Err(e) = s.write_trade(t).await {
                                    tracing::warn!("Failed to write triggered trade: {}", e);
                                }
                            }
                            // Update the triggered stop order's status in DB
                            if let Some((_, fo)) = eng.find_order_by_id(stop_resp.order_id) {
                                if let Err(e) = s.update_order_status(stop_resp.order_id, fo.filled_quantity, &fo.status).await {
                                    tracing::warn!("Failed to update triggered stop status: {}", e);
                                }
                            }
                            // Update maker orders' statuses in DB
                            for t in &stop_resp.trades {
                                if let Some((_, bo)) = eng.find_order_by_id(t.buy_order_id) {
                                    let _ = s.update_order_status(t.buy_order_id, bo.filled_quantity, &bo.status).await;
                                }
                                if let Some((_, so)) = eng.find_order_by_id(t.sell_order_id) {
                                    let _ = s.update_order_status(t.sell_order_id, so.filled_quantity, &so.status).await;
                                }
                            }
                        }
                                        // Record triggered stop order in history
                        {
                            let mut oh = self.order_history.write().await;
                            // Find the final state from matching engine
                            if let Some((_, fo)) = eng.find_order_by_id(stop_resp.order_id) {
                                oh.push(fo.clone());
                            }
                            // Also record maker orders that were filled
                            for t in &stop_resp.trades {
                                if let Some((_, bo)) = eng.find_order_by_id(t.buy_order_id) {
                                    if bo.status == crate::types::OrderStatus::Filled || bo.status == crate::types::OrderStatus::PartiallyFilled {
                                        oh.push(bo.clone());
                                    }
                                }
                                if let Some((_, so)) = eng.find_order_by_id(t.sell_order_id) {
                                    if so.status == crate::types::OrderStatus::Filled || so.status == crate::types::OrderStatus::PartiallyFilled {
                                        oh.push(so.clone());
                                    }
                                }
                            }
                        }
                        // Check for cascading triggers
                        if !stop_resp.trades.is_empty() {
                            let cascade_price = stop_resp.trades.last().unwrap().price;
                            let cascade = eng.check_trigger_orders(&stop_symbol, cascade_price);
                            for c in cascade {
                                tracing::info!("Cascade triggered order {}", c.id);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Stop order execution failed: {}", e);
                    }
                }
            }
        }

        // Record the order's final state in in-memory history
        {
            let mut oh = self.order_history.write().await;
            if let Some((_, fo)) = eng.find_order_by_id(result.order_id) {
                oh.push(fo.clone());
            } else {
                // Order was consumed (e.g., IOC with no fill); reconstruct from result
                oh.push(crate::types::Order {
                    id: result.order_id,
                    user_id: order_user.clone(),
                    symbol: order_symbol.clone(),
                    side: order_side,
                    order_type: order_type,
                    price: None,
                    trigger_price: None,
                    quantity: result.filled_quantity + result.remaining_quantity,
                    filled_quantity: result.filled_quantity,
                    status: result.status,
                    time_in_force: crate::types::TimeInForce::Gtc,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    visible_quantity: None,
                    trailing_delta: None,
                    stop_price: None,
                    oco_sibling_id: None,
                });
            }
        }

        if let Some(ref s) = self.store {
            if let Some((_, fo)) = eng.find_order_by_id(result.order_id) {
                s.update_order_status(result.order_id, fo.filled_quantity, &fo.status).await.ok();
            }
            for t in &result.trades {
                s.write_trade(t).await?;
                if let Some((_, bo)) = eng.find_order_by_id(t.buy_order_id) { s.update_order_status(t.buy_order_id, bo.filled_quantity, &bo.status).await.ok(); }
                if let Some((_, so)) = eng.find_order_by_id(t.sell_order_id) { s.update_order_status(t.sell_order_id, so.filled_quantity, &so.status).await.ok(); }
            }
        }

        // OCO auto-cancel: if the filled order was part of an OCO pair, cancel the sibling
        if !result.trades.is_empty() {
            let oco_result = eng.cancel_oco_sibling(result.order_id);
            if let Some(sibling) = oco_result {
                self.risk.release_for_cancellation(&sibling).await;
                if let Some(ref s) = self.store {
                    let _ = s.record_cancellation(sibling.id, &sibling.symbol).await;
                }
                tracing::info!("OCO sibling {} cancelled after {} filled", sibling.id, result.order_id);
            }
        }

        Ok(result)
    }

    pub async fn cancel_order(&self, symbol: &str, id: Uuid) -> ExchangeResult<Order> {
        let mut eng = self.inner.write().await;
        let order = if symbol.is_empty() { eng.cancel_order_by_id(id)? } else { eng.cancel_order(symbol, id)? };
        self.risk.release_for_cancellation(&order).await;
        if let Some(ref s) = self.store { s.record_cancellation(id, if symbol.is_empty() { &order.symbol } else { symbol }).await?; }
        // Record cancellation in in-memory history
        {
            let mut oh = self.order_history.write().await;
            oh.push(order.clone());
        }
        Ok(order)
    }

    pub async fn get_order(&self, symbol: &str, id: Uuid) -> ExchangeResult<Order> {
        let eng = self.inner.read().await;
        if symbol.is_empty() { match eng.find_order_by_id(id) { Some((_, o)) => Ok(o.clone()), None => Err(err_order_not_found(format!("Not found: {}", id))) } }
        else { Ok(eng.get_order_status(symbol, id)?.clone()) }
    }

    pub async fn get_orderbook(&self, symbol: &str, depth: usize) -> ExchangeResult<OrderBookSnapshot> {
        let eng = self.inner.read().await;
        let book = eng.get_book(symbol).ok_or_else(|| err_order_not_found(format!("Not found: {}", symbol)))?;
        let (bids, asks) = book.snapshot(depth);
        Ok(OrderBookSnapshot { symbol: symbol.into(), bids, asks, timestamp: chrono::Utc::now() })
    }

    pub async fn get_symbols(&self) -> Vec<String> {
        self.inner.read().await.order_books().iter().map(|b| b.symbol.clone()).collect()
    }

    pub async fn get_market_summary(&self, symbol: &str) -> ExchangeResult<MarketSummary> {
        let eng = self.inner.read().await;
        let book = eng.get_book(symbol).ok_or_else(|| err_order_not_found(format!("Not found: {}", symbol)))?;
        let (bids, asks) = book.snapshot(10);
        let bd = bids.iter().map(|l| l.price * l.quantity).sum::<f64>();
        let ad = asks.iter().map(|l| l.price * l.quantity).sum::<f64>();
        Ok(MarketSummary { symbol: symbol.into(), best_bid: book.best_bid(), best_ask: book.best_ask(), spread: book.spread(), bid_depth: bd, ask_depth: ad, order_count: book.order_count(), bid_levels: bids.len(), ask_levels: asks.len() })
    }

    pub async fn is_healthy(&self) -> bool {
        if let Some(ref s) = self.store { s.health_check().await } else { true }
    }

    pub async fn get_best_bid_ask(&self, symbol: &str) -> (Option<f64>, Option<f64>) {
        let eng = self.inner.read().await;
        match eng.get_book(symbol) {
            Some(book) => (book.best_bid(), book.best_ask()),
            None => (None, None),
        }
    }

    pub async fn get_open_orders(&self, uid: &str) -> Vec<crate::types::Order> {
        let eng = self.inner.read().await;
        let mut orders = Vec::new();
        for book in eng.order_books() {
            for (_id, order) in book.all_orders() {
                if order.user_id == uid && order.is_active() {
                    orders.push(order.clone());
                }
            }
        }
        // Also include trigger orders
        if let Ok(triggers) = eng.get_trigger_orders(uid) {
            orders.extend(triggers);
        }
        orders
    }

    pub async fn get_order_history(&self, uid: &str, limit: i64, offset: usize) -> Vec<crate::types::Order> {
        // Always try DB first when available (it has full history)
        if let Some(ref store) = self.store {
            if let Ok(orders) = store.get_order_history(uid, limit, offset).await {
                return orders;
            }
        }
        // Fallback to in-memory order history
        let oh = self.order_history.read().await;
        oh.iter()
            .rev()
            .filter(|o| o.user_id == uid)
            .skip(offset)
            .take(limit as usize)
            .cloned()
            .collect()
    }

    pub async fn get_recent_trades(&self, symbol: &str, limit: i64) -> Vec<crate::types::Trade> {
        // Always try DB first when available (it has full history)
        if let Some(ref s) = self.store {
            if let Ok(trades) = s.get_recent_trades(symbol, limit).await {
                return trades;
            }
        }
        // Fallback to in-memory trade store
        let trades = self.trades.read().await;
        let filtered: Vec<crate::types::Trade> = if symbol.is_empty() || symbol == "*" {
            trades.iter().rev().cloned().collect()
        } else {
            trades.iter().rev().filter(|t| t.symbol == symbol).cloned().collect()
        };
        filtered.into_iter().take(limit as usize).collect()
    }

    pub async fn get_candles(&self, symbol: &str, interval: &str, limit: usize) -> Vec<Candle> {
        self.candles.read().await.get_candles(symbol, interval, limit)
    }

    pub async fn amend_order(&self, id: Uuid, new_price: Option<f64>, new_quantity: Option<f64>) -> ExchangeResult<Order> {
        let mut eng = self.inner.write().await;
        // Cancel the old order
        let old_order = eng.cancel_order_by_id(id)?;
        self.risk.release_for_cancellation(&old_order).await;
        if let Some(ref s) = self.store { s.record_cancellation(id, &old_order.symbol).await?; }

        // Create amended order preserving original fields
        let mut amended = old_order.clone();
        amended.id = Uuid::new_v4();
        amended.status = crate::types::OrderStatus::Pending;
        amended.filled_quantity = 0.0;
        amended.updated_at = chrono::Utc::now();
        if let Some(p) = new_price { amended.price = Some(p); }
        if let Some(q) = new_quantity { amended.quantity = q; }

        // Record old order as cancelled in history
        {
            let mut cancelled = old_order.clone();
            cancelled.status = crate::types::OrderStatus::Cancelled;
            let mut oh = self.order_history.write().await;
            oh.push(cancelled);
        }

        // Submit the amended order
        drop(eng); // release write lock
        let result = self.place_order(amended).await?;
        // Find the order we just placed
        let eng2 = self.inner.read().await;
        let order = eng2.find_order_by_id(result.order_id)
            .map(|(_, o)| o.clone())
            .unwrap_or(old_order);
        Ok(order)
    }

    // ── Futures Methods ────────────────────────────────────

    pub async fn set_leverage(&self, uid: &str, symbol: &str, lev: u32) -> ExchangeResult<u32> {
        self.futures.set_leverage(uid, symbol, lev)
    }

    pub async fn get_leverage(&self, uid: &str, symbol: &str) -> u32 {
        self.futures.get_leverage(uid, symbol)
    }

    pub async fn set_margin_mode(&self, uid: &str, symbol: &str, mode: crate::types::MarginMode) {
        self.futures.set_margin_mode(uid, symbol, mode);
    }

    pub async fn get_margin_mode(&self, uid: &str, symbol: &str) -> crate::types::MarginMode {
        self.futures.get_margin_mode(uid, symbol)
    }

    pub async fn set_position_mode(&self, uid: &str, mode: crate::types::PositionMode) {
        self.futures.set_position_mode(uid, mode);
    }

    pub async fn get_position_mode(&self, uid: &str) -> crate::types::PositionMode {
        self.futures.get_position_mode(uid)
    }

    pub async fn get_positions(&self, uid: &str) -> Vec<crate::types::PositionInfo> {
        self.futures.get_all_positions(uid)
    }

    pub async fn get_funding_rate(&self, symbol: &str) -> crate::types::FundingRateInfo {
        self.futures.get_funding_rate(symbol)
    }

    pub async fn check_liquidation(&self, uid: &str, symbol: &str) -> Option<crate::types::PositionInfo> {
        self.futures.check_liquidation(uid, symbol)
    }

    pub async fn get_exchange_info(&self) -> ExchangeInfo {
        let markets = crate::types::default_markets();
        ExchangeInfo {
            timezone: "UTC".into(),
            server_time: chrono::Utc::now().timestamp_millis(),
            symbols: markets,
            rate_limits: vec![
                crate::types::RateLimitInfo { rate_limit_type: "REQUEST_WEIGHT".into(), interval: "MINUTE".into(), interval_num: 1, limit: 1200 },
                crate::types::RateLimitInfo { rate_limit_type: "ORDERS".into(), interval: "SECOND".into(), interval_num: 10, limit: 50 },
            ],
        }
    }

    pub async fn get_ticker_24hr(&self, symbol: &str) -> ExchangeResult<Ticker24hr> {
        let eng = self.inner.read().await;
        let book = eng.get_book(symbol).ok_or_else(|| err_order_not_found(format!("Not found: {}", symbol)))?;
        let best_bid = book.best_bid().unwrap_or(0.0);
        let best_ask = book.best_ask().unwrap_or(0.0);
        let trades = self.trades.read().await;
        let sym_trades: Vec<_> = trades.iter().filter(|t| t.symbol == symbol).collect();
        let count = sym_trades.len() as u64;
        let volume = sym_trades.iter().map(|t| t.quantity).sum();
        let quote_volume = sym_trades.iter().map(|t| t.total).sum();
        let low = sym_trades.iter().map(|t| t.price).fold(f64::MAX, |a, b| a.min(b));
        let high = sym_trades.iter().map(|t| t.price).fold(f64::MIN, |a, b| a.max(b));
        let low = if low == f64::MAX { 0.0 } else { low };
        let high = if high == f64::MIN { 0.0 } else { high };
        let last_price = sym_trades.last().map(|t| t.price).unwrap_or(0.0);
        let open_price = sym_trades.first().map(|t| t.price).unwrap_or(last_price);
        let price_change = last_price - open_price;
        let price_change_percent = if open_price != 0.0 { (price_change / open_price) * 100.0 } else { 0.0 };
        Ok(Ticker24hr {
            symbol: symbol.into(), price_change, price_change_percent,
            last_price, bid_price: best_bid, ask_price: best_ask,
            high_price: high, low_price: low, volume, quote_volume,
            count, first_trade_id: "".into(), last_trade_id: "".into(),
            open_time: chrono::Utc::now(), close_time: chrono::Utc::now(),
        })
    }

    pub async fn get_all_tickers_24hr(&self) -> Vec<Ticker24hr> {
        let symbols = self.get_symbols().await;
        let mut tickers = Vec::new();
        for sym in symbols {
            if let Ok(t) = self.get_ticker_24hr(&sym).await {
                tickers.push(t);
            }
        }
        tickers
    }

    pub async fn place_oco_order(&self, req: OcoRequest) -> ExchangeResult<OcoResponse> {
        // Both legs are on the SAME side.
        // For a long position (buy): OCO has a sell limit above (take profit) + sell stop below (stop loss)
        // For a short position (sell): OCO has a buy limit below (take profit) + buy stop above (stop loss)
        // The side field in OcoRequest indicates the position side; both legs use the opposite side.
        let leg_side = req.side.opposite();
        let limit_order = crate::types::Order::new_limit(
            req.user_id.clone(), req.symbol.clone(), leg_side, req.price, req.quantity,
        );
        // Create the stop-limit (stop-loss) leg
        let stop_price = req.stop_price;
        let stop_limit_price = req.stop_limit_price.unwrap_or(stop_price);
        let stop_order = crate::types::Order::new_stop_limit(
            req.user_id.clone(), req.symbol.clone(), leg_side, stop_price, stop_limit_price, req.quantity,
        );

        let oco_id = Uuid::new_v4();
        let o1 = limit_order;
        let o2 = stop_order;

        // Place both orders first — only register the pair once both definitely exist
        let resp1 = self.place_order(o1).await.map_err(|_| err_internal("OCO limit leg failed"))?;
        let resp2 = self.place_order(o2).await.map_err(|_| err_internal("OCO stop leg failed"))?;

        // Register the OCO pair in the matching engine for auto-cancel on subsequent fills
        {
            let mut eng = self.inner.write().await;
            eng.register_oco_pair(oco_id, resp1.order_id, resp2.order_id);
        }

        Ok(OcoResponse {
            oco_id,
            orders: vec![resp1, resp2],
            message: "OCO order placed successfully".into(),
        })
    }

    // ── Background Tasks ───────────────────────────────────

    /// Spawn a background task that runs every 30 seconds:
    /// 1. Updates mark prices from orderbook mid-prices
    /// 2. Adjusts funding rates based on long/short imbalance
    /// 3. Settles funding payments between long and short position holders
    /// 4. Liquidates positions that have breached their liquidation price
    pub fn run_background_tasks(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            // Tick immediately on start, then every 30s
            interval.tick().await;
            loop {
                interval.tick().await;
                this.background_cycle().await;
            }
        });
        tracing::info!("Background tasks spawned (30s interval)");
    }

    async fn background_cycle(&self) {
        let markets = crate::types::default_markets();
        for mkt in &markets {
            let symbol = &mkt.symbol;

            // 1. Get mark price from orderbook mid-price
            let (bid, ask) = self.get_best_bid_ask(symbol).await;
            let mark_price = match (bid, ask) {
                (Some(b), Some(a)) => Some((b + a) / 2.0),
                (Some(b), None) => Some(b),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            };
            let mark = match mark_price {
                Some(p) if p > 0.0 => p,
                _ => continue, // No valid price for this market yet
            };

            self.futures.update_mark_price(symbol, mark);

            // 2. Update funding rate from long/short skew
            let (long_interest, short_interest) = self.futures.get_long_short_interest(symbol);
            self.futures.update_funding_rate(symbol, long_interest, short_interest);

            // 3. Check if funding settlement is due
            let funding_info = self.futures.get_funding_rate(symbol);
            let now = chrono::Utc::now();
            if now >= funding_info.next_funding_time {
                self.settle_funding_for_symbol(symbol, mark).await;
            }

            // 4. Liquidate any positions that need it
            self.liquidate_positions_for_symbol(symbol).await;
        }
    }

    /// Settle funding payments between long and short position holders.
    async fn settle_funding_for_symbol(&self, symbol: &str, mark_price: f64) {
        let rate = self.futures.get_funding_rate(symbol).funding_rate;
        if rate.abs() < 0.000001 {
            self.futures.advance_funding_timestamp(symbol);
            return;
        }

        let holders = self.futures.get_position_holders_for_symbol(symbol);
        if holders.is_empty() {
            self.futures.advance_funding_timestamp(symbol);
            return;
        }

        let mut total_long_payment = 0.0;
        let mut total_short_payment = 0.0;

        // rate > 0: longs pay shorts
        // rate < 0: shorts pay longs
        for (uid, side, size, _entry, _lev, _margin) in &holders {
            let payment = (rate * size * mark_price).abs();
            if rate > 0.0 {
                match side {
                    PositionSide::Long => {
                        self.risk.deduct(uid, "USD", payment);
                        total_long_payment += payment;
                    }
                    PositionSide::Short => {
                        self.risk.deposit(uid, "USD", payment);
                        total_short_payment += payment;
                    }
                }
            } else {
                match side {
                    PositionSide::Long => {
                        self.risk.deposit(uid, "USD", payment);
                        total_long_payment += payment;
                    }
                    PositionSide::Short => {
                        self.risk.deduct(uid, "USD", payment);
                        total_short_payment += payment;
                    }
                }
            }
        }

        self.futures.advance_funding_timestamp(symbol);
        tracing::info!(
            "Funding settlement for {}: rate={:.6}%, longs paid {:.2}, shorts paid {:.2}",
            symbol, rate * 100.0, total_long_payment, total_short_payment
        );
    }

    /// Check all positions for a symbol and liquidate any that have breached their liquidation price.
    async fn liquidate_positions_for_symbol(&self, symbol: &str) {
        let holders = self.futures.get_position_holders_for_symbol(symbol);
        for (uid, _side, _size, _entry, _lev, _margin) in &holders {
            if let Some(pos) = self.futures.check_liquidation(uid, symbol) {
                tracing::warn!("LIQUIDATING {:?} position for {} on {} (mark={}, liq={})",
                    pos.side, uid, symbol, pos.mark_price, pos.liquidation_price);

                match self.futures.liquidate_position(uid, symbol) {
                    Ok(liquidated) => {
                        for liq_pos in &liquidated {
                            // The user loses their margin as a liquidation penalty
                            let loss = liq_pos.margin.max(1.0); // minimum $1 loss
                            self.risk.deduct(uid, "USD", loss);
                            tracing::info!("  -> {} USD margin seized from {} for {}", loss, uid, symbol);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Liquidation execution failed for {} {}: {}", uid, symbol, e);
                    }
                }
            }
        }
    }

    pub async fn get_portfolio(&self, uid: &str) -> serde_json::Value {
        let balances = self.risk.get_all_balances(uid);
        let mut positions = Vec::new();
        // Detect all symbols the user has positions in
        let mut symbols = std::collections::HashSet::new();
        for (asset, _bal) in &balances {
            // Check if there are any known trading pairs for this asset
            let syms = self.inner.read().await;
            for book in syms.order_books() {
                let (base, quote) = match book.symbol.split_once('/') {
                    Some(p) => (p.0.to_string(), p.1.to_string()),
                    None => continue,
                };
                if base == *asset || quote == *asset {
                    symbols.insert(book.symbol.clone());
                }
            }
        }
        for sym in &symbols {
            let pos = self.risk.get_pos(uid, sym);
            let (bid, ask) = self.get_best_bid_ask(sym).await;
            let mid = match (bid, ask) {
                (Some(b), Some(a)) => Some((b + a) / 2.0),
                _ => None,
            };
            positions.push(serde_json::json!({
                "symbol": sym,
                "net": pos.net,
                "open_buy": pos.open_buy,
                "open_sell": pos.open_sell,
                "current_price": mid,
                "unrealized_pnl": mid.map(|m| m * pos.net),
            }));
        }
        let bal_list: Vec<serde_json::Value> = balances.into_iter().map(|(a, b)| serde_json::json!({
            "asset": a,
            "available": b.avail,
            "locked": b.locked,
            "total": b.total(),
        })).collect();
        let open_orders = self.get_open_orders(uid).await;
        serde_json::json!({
            "user_id": uid,
            "balances": bal_list,
            "positions": positions,
            "open_orders": open_orders,
            "open_orders_count": open_orders.len(),
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct MarketSummary { pub symbol: String, pub best_bid: Option<f64>, pub best_ask: Option<f64>, pub spread: Option<f64>, pub bid_depth: f64, pub ask_depth: f64, pub order_count: usize, pub bid_levels: usize, pub ask_levels: usize }
