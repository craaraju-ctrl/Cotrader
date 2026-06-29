use std::convert::Infallible;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use std::sync::Arc;
use dashmap::DashMap;
use axum::{
    middleware,
    extract::Extension,
};

use crate::auth::ApiKeyPair;
use crate::engine::ExchangeEngine;
use crate::rat::types::RatEvent;
use crate::types::PlaceOrderRequest;

use super::ws::{self, WsEvent};

#[derive(Clone)]
pub struct AppState {
    pub engine: ExchangeEngine,
    pub ws_tx: broadcast::Sender<WsEvent>,
    pub rat_tx: broadcast::Sender<RatEvent>,
    pub api_keys: Arc<DashMap<String, ApiKeyPair>>,
}

pub fn create_router(state: AppState) -> Router {
    // Routes that don't require authentication
    let public_routes = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/markets", get(get_mkts))
        .route("/api/v1/orderbook", get(get_ob))
        .route("/api/v1/markets/query", get(get_ms))
        .route("/api/v1/trades", get(get_trades))
        .route("/api/v1/candles", get(get_candles))
        .route("/api/v1/exchange/info", get(get_exchange_info))
        .route("/api/v1/ticker/24hr", get(get_ticker_24hr))
        .route("/api/v1/ai/signal/:symbol", get(ai_signal))
        .route("/api/v1/admin/config", get(get_config))
        .route("/api/v1/auth/keys", post(generate_api_key))
        .route("/api/v1/ws", get(ws::ws_handler))
        .route("/api/v1/ws/depth/:symbol", get(ws::ws_depth_handler))
        .route("/api/v1/stream", get(sse_handler))

        // ── RAT (Remote Access Terminal) Stream ──
        .route("/api/v1/rat/stream", get(crate::rat::stream::rat_ws_handler))
        .route("/api/v1/rat/snapshot", get(crate::rat::stream::rat_rest_snapshot));

    // Routes that require HMAC authentication
    let auth_routes = Router::new()
        .route("/api/v1/orders", post(place_order))
        .route("/api/v1/order/oco", post(place_oco_order))
        .route("/api/v1/orders/:id", get(get_order))
        .route("/api/v1/orders/:id", delete(cancel_order))
        .route("/api/v1/orders/:id/amend", post(amend_order))
        .route("/api/v1/orders/open/:uid", get(get_open_orders))
        .route("/api/v1/orders/history/:uid", get(get_order_history))
        .route("/api/v1/balances/:uid", get(get_bals))
        .route("/api/v1/portfolio/:uid", get(get_portfolio))
        .route("/api/v1/futures/leverage", post(set_leverage))
        .route("/api/v1/futures/leverage/:uid/:symbol", get(get_leverage))
        .route("/api/v1/futures/margin_mode", post(set_margin_mode))
        .route("/api/v1/futures/margin_mode/:uid/:symbol", get(get_margin_mode))
        .route("/api/v1/futures/position_mode", post(set_position_mode))
        .route("/api/v1/futures/position_mode/:uid", get(get_position_mode))
        .route("/api/v1/futures/positions/:uid", get(get_positions))
        .route("/api/v1/futures/funding/:symbol", get(get_funding_rate_route))
        .route("/api/v1/futures/liquidation/:uid/:symbol", get(check_liquidation))
        .route("/api/v1/withdraw", post(withdraw))
        .route("/api/v1/deposit", post(deposit))
        .route("/api/v1/orchestra/metrics", get(get_orchestra_metrics))
        .route("/api/v1/orchestra/config", post(update_orchestra_config))
        .layer(middleware::from_fn_with_state(state.clone(), crate::auth::middleware::auth_middleware));

    public_routes.merge(auth_routes).with_state(state)
}

// ── Orchestra Management ─────────────────────────────────

async fn get_orchestra_metrics(
    State(_s): State<AppState>,
) -> impl IntoResponse {
    // In a full implementation, the orchestra handle would be stored
    // in AppState and queried for live metrics.
    // For now, return a placeholder — the metrics are logged and streamed via RAT.
    (StatusCode::OK, Json(serde_json::json!({
        "status": "running",
        "symbols": ["BTC/USD", "ETH/USD"],
        "message": "Orchestra metrics available via RAT stream (/api/v1/rat/stream)"
    })))
}

async fn update_orchestra_config(
    State(_s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Json(r): Json<serde_json::Value>,
) -> impl IntoResponse {
    tracing::info!("Orchestra config update from user {}: {:?}", auth.user_id, r);
    (StatusCode::OK, Json(serde_json::json!({
        "status": "acknowledged",
        "config": r
    })))
}

// ── SSE ────────────────────────────────────────────────────

async fn sse_handler(
    State(s): State<AppState>,
    Query(p): Query<SseQ>,
) -> Sse<impl futures::Stream<Item = Result<SseEvent, Infallible>>> {
    let filter: Option<Vec<String>> = p.symbols.as_ref().map(|s| s.split(',').map(|s| s.trim().to_string()).collect());
    let mut rx = s.ws_tx.subscribe();
    let f = filter.clone();
    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            let dominated = match &event {
                WsEvent::Trade { symbol, .. } | WsEvent::OrderBookUpdate { symbol, .. } | WsEvent::DepthUpdate { symbol, .. } => f.as_ref().map_or(true, |f| f.contains(symbol)),
                WsEvent::OrderUpdate { .. } | WsEvent::Pong => true,
            };
            if dominated { if let Ok(json) = serde_json::to_string(&event) { yield Ok(SseEvent::default().data(json)); } }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct SseQ { symbols: Option<String> }

// ── AI Signal ──────────────────────────────────────────────

async fn ai_signal(State(s): State<AppState>, Path(symbol): Path<String>) -> impl IntoResponse {
    let (best_bid, best_ask) = s.engine.get_best_bid_ask(&symbol).await;
    let mid = match (best_bid, best_ask) {
        (Some(b), Some(a)) => Some((b + a) / 2.0),
        _ => None,
    };
    let spread = match (best_bid, best_ask) {
        (Some(b), Some(a)) => Some(a - b),
        _ => None,
    };

    let mut reasons = Vec::new();
    if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
        let spread_pct = ((ask - bid) / ((bid + ask) / 2.0)) * 100.0;
        if spread_pct > 1.0 {
            reasons.push(format!("Wide spread ({:.2}%) — consider market making", spread_pct));
        } else if spread_pct < 0.1 {
            reasons.push(format!("Tight spread ({:.2}%) — high liquidity", spread_pct));
        } else {
            reasons.push(format!("Normal spread ({:.2}%)", spread_pct));
        }
    }

    let orderbook = s.engine.get_orderbook(&symbol, 5).await.ok();
    let order_count = orderbook.as_ref().map(|o| o.bids.len() + o.asks.len()).unwrap_or(0);

    (StatusCode::OK, Json(serde_json::json!({
        "symbol": symbol,
        "best_bid": best_bid,
        "best_ask": best_ask,
        "mid": mid,
        "spread": spread,
        "orderbook_depth": order_count,
        "signal": if reasons.is_empty() { "NO_DATA" } else { "ANALYSIS" },
        "reasons": reasons,
        "timestamp": chrono::Utc::now()
    })))
}

// ── Orders ─────────────────────────────────────────────────

async fn place_order(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Json(r): Json<PlaceOrderRequest>,
) -> impl IntoResponse {
    if auth.user_id != r.user_id {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let o = match r.order_type {
        crate::types::OrderType::Limit => {
            let mut o = crate::types::Order::new_limit(r.user_id, r.symbol.clone(), r.side, r.price.unwrap_or(0.0), r.quantity);
            o.time_in_force = r.time_in_force;
            o
        },
        crate::types::OrderType::Market => crate::types::Order::new_market(r.user_id, r.symbol, r.side, r.quantity),
        crate::types::OrderType::StopLoss => crate::types::Order::new_stop_loss(
            r.user_id, r.symbol, r.side, r.trigger_price.unwrap_or(0.0), r.quantity,
        ),
        crate::types::OrderType::StopLimit => crate::types::Order::new_stop_limit(
            r.user_id, r.symbol, r.side, r.trigger_price.unwrap_or(0.0), r.price.unwrap_or(0.0), r.quantity,
        ),
        crate::types::OrderType::TakeProfit => {
            let mut o = crate::types::Order::new_take_profit(
                r.user_id, r.symbol, r.side, r.trigger_price.unwrap_or(0.0), r.quantity,
            );
            o.time_in_force = r.time_in_force;
            o
        },
        crate::types::OrderType::TakeProfitLimit => {
            let mut o = crate::types::Order::new_take_profit_limit(
                r.user_id, r.symbol, r.side, r.trigger_price.unwrap_or(0.0), r.price.unwrap_or(0.0), r.quantity,
            );
            o.time_in_force = r.time_in_force;
            o
        },
        crate::types::OrderType::TrailingStop => {
            let mut o = crate::types::Order::new_trailing_stop(
                r.user_id, r.symbol, r.side, r.trailing_delta.unwrap_or(0.0), r.quantity, r.trigger_price.unwrap_or(0.0),
            );
            o.visible_quantity = r.visible_quantity;
            o.time_in_force = r.time_in_force;
            o
        },
    };

    // Apply Iceberg modifier after construction
    let mut o = o;
    if let Some(vq) = r.visible_quantity {
        if o.order_type == crate::types::OrderType::Limit {
            o.visible_quantity = Some(vq);
        }
    }

    match s.engine.place_order(o).await {
        Ok(resp) => {
            for trade in &resp.trades {
                ws::broadcast_trade(&s.ws_tx, trade).await;
                crate::rat::stream::broadcast_rat_trade(&s.rat_tx, trade).await;
            }

            // Broadcast order update via WS
            let _ = s.ws_tx.send(WsEvent::OrderUpdate {
                order_id: resp.order_id.to_string(),
                status: format!("{:?}", resp.status),
                filled_quantity: resp.trades.iter().map(|t| t.quantity).sum(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });

            // Broadcast orderbook update
            if !resp.trades.is_empty() {
                let sym = &resp.trades[0].symbol;
                let (bid, ask) = s.engine.get_best_bid_ask(sym).await;
                let scaled_bid = ws::scale_price_opt(bid);
                let scaled_ask = ws::scale_price_opt(ask);
                let spread = match (scaled_bid, scaled_ask) { (Some(b), Some(a)) => Some(a - b), _ => None };
                let _ = s.ws_tx.send(WsEvent::OrderBookUpdate {
                    symbol: sym.clone(),
                    best_bid: scaled_bid,
                    best_ask: scaled_ask,
                    spread,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
                ws::broadcast_depth(&s.ws_tx, &s.engine, sym).await;

                // Broadcast orderbook snapshot to RAT stream
                crate::rat::stream::broadcast_rat_orderbook(&s.rat_tx, sym, &s.engine).await;
            }

            (StatusCode::OK, Json(serde_json::json!(resp))).into_response()
        }
        Err(e) => {
            (e.to_http_status(), Json(e.binance_json())).into_response()
        }
    }
}

async fn get_order(State(s): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match s.engine.get_order("", id).await {
        Ok(o) => (StatusCode::OK, Json(serde_json::json!(o))).into_response(),
        Err(e) => { (e.to_http_status(), Json(e.binance_json())).into_response() }
    }
}

async fn cancel_order(State(s): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match s.engine.cancel_order("", id).await {
        Ok(o) => {
            // Broadcast cancellation via WS
            let _ = s.ws_tx.send(WsEvent::OrderUpdate {
                order_id: o.id.to_string(),
                status: "Cancelled".into(),
                filled_quantity: o.filled_quantity,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
            // Broadcast updated orderbook via WS
            let (bid, ask) = s.engine.get_best_bid_ask(&o.symbol).await;
            let scaled_bid = ws::scale_price_opt(bid);
            let scaled_ask = ws::scale_price_opt(ask);
            let spread = match (scaled_bid, scaled_ask) { (Some(b), Some(a)) => Some(a - b), _ => None };
            let _ = s.ws_tx.send(WsEvent::OrderBookUpdate {
                symbol: o.symbol.clone(), best_bid: scaled_bid, best_ask: scaled_ask, spread,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
            ws::broadcast_depth(&s.ws_tx, &s.engine, &o.symbol).await;

            // Broadcast snapshot to RAT
            crate::rat::stream::broadcast_rat_orderbook(&s.rat_tx, &o.symbol, &s.engine).await;

            (StatusCode::OK, Json(serde_json::json!(o))).into_response()
        }
        Err(e) => { (e.to_http_status(), Json(e.binance_json())).into_response() }
    }
}

// ── Exchange Info ─────────────────────────────────────────

async fn get_exchange_info(State(s): State<AppState>) -> impl IntoResponse {
    let info = s.engine.get_exchange_info().await;
    (StatusCode::OK, Json(info))
}

// ── 24hr Ticker ────────────────────────────────────────────

async fn get_ticker_24hr(State(s): State<AppState>, Query(p): Query<TickerQ>) -> impl IntoResponse {
    let sym = p.symbol.as_deref().unwrap_or("");
    if sym.is_empty() {
        let tickers = s.engine.get_all_tickers_24hr().await;
        return (StatusCode::OK, Json(tickers)).into_response();
    }
    match s.engine.get_ticker_24hr(sym).await {
        Ok(t) => (StatusCode::OK, Json(serde_json::json!(t))).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"code": -1013, "msg": "Symbol not found"}))).into_response(),
    }
}

#[derive(Deserialize)]
struct TickerQ { symbol: Option<String> }

// ── Futures ───────────────────────────────────────────────

async fn set_leverage(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Json(r): Json<crate::types::LeverageRequest>,
) -> impl IntoResponse {
    if auth.user_id != r.user_id {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    match s.engine.set_leverage(&r.user_id, &r.symbol, r.leverage).await {
        Ok(lev) => (StatusCode::OK, Json(serde_json::json!({"user_id": r.user_id, "symbol": r.symbol, "leverage": lev}))).into_response(),
        Err(e) => { (e.to_http_status(), Json(e.binance_json())).into_response() }
    }
}

async fn get_leverage(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Path((uid, symbol)): Path<(String, String)>,
) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let lev = s.engine.get_leverage(&uid, &symbol).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": uid, "symbol": symbol, "leverage": lev}))).into_response()
}

async fn set_margin_mode(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Json(r): Json<crate::types::MarginModeRequest>,
) -> impl IntoResponse {
    if auth.user_id != r.user_id {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    s.engine.set_margin_mode(&r.user_id, &r.symbol, r.margin_mode).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": r.user_id, "symbol": r.symbol, "margin_mode": format!("{:?}", r.margin_mode)}))).into_response()
}

async fn get_margin_mode(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Path((uid, symbol)): Path<(String, String)>,
) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let mode = s.engine.get_margin_mode(&uid, &symbol).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": uid, "symbol": symbol, "margin_mode": format!("{:?}", mode)}))).into_response()
}

async fn set_position_mode(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Json(r): Json<crate::types::PositionModeRequest>,
) -> impl IntoResponse {
    if auth.user_id != r.user_id {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    s.engine.set_position_mode(&r.user_id, r.position_mode).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": r.user_id, "position_mode": format!("{:?}", r.position_mode)}))).into_response()
}

async fn get_position_mode(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Path(uid): Path<String>,
) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let mode = s.engine.get_position_mode(&uid).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": uid, "position_mode": format!("{:?}", mode)}))).into_response()
}

async fn get_positions(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Path(uid): Path<String>,
) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let positions = s.engine.get_positions(&uid).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": uid, "positions": positions, "count": positions.len()}))).into_response()
}

async fn get_funding_rate_route(
    State(s): State<AppState>,
    Path(symbol): Path<String>,
) -> impl IntoResponse {
    let rate = s.engine.get_funding_rate(&symbol).await;
    (StatusCode::OK, Json(serde_json::json!(rate))).into_response()
}

async fn check_liquidation(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Path((uid, symbol)): Path<(String, String)>,
) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    match s.engine.check_liquidation(&uid, &symbol).await {
        Some(pos) => (StatusCode::OK, Json(serde_json::json!({"liquidation_required": true, "position": pos}))).into_response(),
        None => (StatusCode::OK, Json(serde_json::json!({"liquidation_required": false}))).into_response(),
    }
}

// ── OCO Order ──────────────────────────────────────────────

async fn place_oco_order(
    State(s): State<AppState>,
    Extension(auth): Extension<crate::auth::middleware::AuthUser>,
    Json(r): Json<crate::types::OcoRequest>,
) -> impl IntoResponse {
    if auth.user_id != r.user_id {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    match s.engine.place_oco_order(r).await {
        Ok(resp) => (StatusCode::OK, Json(serde_json::json!(resp))).into_response(),
        Err(e) => {
            let st = e.to_http_status();
            (st, Json(serde_json::json!({"code": e.code.code(), "msg": e.msg}))).into_response()
        }
    }
}

// ── Market Data ────────────────────────────────────────────

async fn get_ob(State(s): State<AppState>, Query(p): Query<OBQ>) -> impl IntoResponse {
    let sym = p.symbol.as_deref().unwrap_or("");
    if sym.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Missing symbol"}))).into_response(); }
    match s.engine.get_orderbook(sym, p.depth.unwrap_or(10).min(100)).await {
        Ok(snap) => (StatusCode::OK, Json(serde_json::json!(snap))).into_response(),
        Err(e) => { (e.to_http_status(), Json(e.binance_json())).into_response() }
    }
}

async fn get_mkts(State(s): State<AppState>) -> impl IntoResponse {
    let syms = s.engine.get_symbols().await;
    (StatusCode::OK, Json(serde_json::json!({"symbols": syms, "count": syms.len()})))
}

async fn get_ms(State(s): State<AppState>, Query(p): Query<MQ>) -> impl IntoResponse {
    let sym = p.symbol.as_deref().unwrap_or("");
    if sym.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Missing symbol"}))).into_response(); }
    match s.engine.get_market_summary(sym).await {
        Ok(sum) => (StatusCode::OK, Json(serde_json::json!(sum))).into_response(),
        Err(e) => { (e.to_http_status(), Json(e.binance_json())).into_response() }
    }
}

async fn get_trades(State(s): State<AppState>, Query(p): Query<TradeQ>) -> impl IntoResponse {
    let sym = p.symbol.as_deref().unwrap_or("");
    if sym.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Missing symbol"}))).into_response(); }
    let limit = p.limit.unwrap_or(50).min(500) as i64;
    let trades = s.engine.get_recent_trades(sym, limit).await;
    (StatusCode::OK, Json(serde_json::json!({"symbol": sym, "trades": trades, "count": trades.len()}))).into_response()
}

// ── Balances ───────────────────────────────────────────────

async fn get_bals(State(s): State<AppState>, Extension(auth): Extension<crate::auth::middleware::AuthUser>, Path(uid): Path<String>) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let risk = s.engine.risk_engine();
    let bals = risk.get_all_balances(&uid);
    let f: Vec<BalR> = bals.into_iter().map(|(a, e)| BalR { asset: a, available: e.avail, locked: e.locked, total: e.total() }).collect();
    (StatusCode::OK, Json(serde_json::json!({"user_id": uid, "balances": f}))).into_response()
}

async fn deposit(State(s): State<AppState>, Extension(auth): Extension<crate::auth::middleware::AuthUser>, Json(r): Json<DepR>) -> impl IntoResponse {
    if auth.user_id != r.user_id {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    if r.amount <= 0.0 { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Amount must be positive"}))).into_response(); }
    let risk = s.engine.risk_engine();
    risk.deposit(&r.user_id, &r.asset, r.amount);
    // Broadcast balance update to RAT
    let bal_avail = risk.get_balance(&r.user_id, &r.asset).avail;
    let bal_locked = risk.get_balance(&r.user_id, &r.asset).locked;
    crate::rat::stream::broadcast_rat_balance(&s.rat_tx, &r.user_id, &r.asset, bal_avail, bal_locked).await;
    let b = risk.get_balance(&r.user_id, &r.asset);
    (StatusCode::OK, Json(serde_json::json!({"user_id": r.user_id, "asset": r.asset, "deposited": r.amount, "available": b.avail, "locked": b.locked, "total": b.total()}))).into_response()
}

// ── Withdraw ────────────────────────────────────────────────

async fn withdraw(State(s): State<AppState>, Extension(auth): Extension<crate::auth::middleware::AuthUser>, Json(r): Json<DepR>) -> impl IntoResponse {
    if auth.user_id != r.user_id {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    if r.amount <= 0.0 { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Amount must be positive"}))).into_response(); }
    let risk = s.engine.risk_engine();
    let b = risk.get_balance(&r.user_id, &r.asset);
    if b.avail < r.amount { return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Insufficient balance"}))).into_response(); }
    risk.deduct(&r.user_id, &r.asset, r.amount);
    // Broadcast balance update to WS
    let _ = s.ws_tx.send(super::ws::WsEvent::OrderUpdate {
        order_id: String::new(),
        status: format!("Withdrew {} {}", r.amount, r.asset),
        filled_quantity: 0.0,
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    // Broadcast to RAT
    crate::rat::stream::broadcast_rat_balance(&s.rat_tx, &r.user_id, &r.asset, b.avail - r.amount, b.locked).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": r.user_id, "asset": r.asset, "withdrawn": r.amount, "remaining": b.avail - r.amount}))).into_response()
}

// ── Open Orders ─────────────────────────────────────────────

async fn get_open_orders(State(s): State<AppState>, Extension(auth): Extension<crate::auth::middleware::AuthUser>, Path(uid): Path<String>) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let orders = s.engine.get_open_orders(&uid).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": uid, "orders": orders, "count": orders.len()}))).into_response()
}

async fn get_order_history(State(s): State<AppState>, Extension(auth): Extension<crate::auth::middleware::AuthUser>, Path(uid): Path<String>, Query(p): Query<PageQ>) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let limit = p.limit.unwrap_or(50).min(500) as i64;
    let offset = p.offset.unwrap_or(0);
    let orders = s.engine.get_order_history(&uid, limit, offset).await;
    (StatusCode::OK, Json(serde_json::json!({"user_id": uid, "orders": orders, "count": orders.len(), "limit": limit, "offset": offset}))).into_response()
}

// ── Health ─────────────────────────────────────────────────

async fn health(State(s): State<AppState>) -> impl IntoResponse {
    let db = s.engine.is_healthy().await;
    let code = if db { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (code, Json(serde_json::json!({
        "status": if db {"healthy"} else {"degraded"},
        "service": "tresdo-exchange",
        "version": env!("CARGO_PKG_VERSION"),
        "features": [
            "rest-api", "websocket", "sse", "ai-signal",
            "risk-engine", "persistence", "rat-stream",
            "multi-agent-orchestra", "memory-agent",
        ],
        "db_connected": db,
        "timestamp": chrono::Utc::now()
    })))
}

// ── Candles ────────────────────────────────────────────────

async fn get_candles(State(s): State<AppState>, Query(p): Query<CandleQ>) -> impl IntoResponse {
    let sym = p.symbol.as_deref().unwrap_or("");
    if sym.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Missing symbol"}))).into_response(); }
    let interval = p.interval.unwrap_or("1m".into());
    let limit = p.limit.unwrap_or(50).min(500);
    let candles = s.engine.get_candles(sym, &interval, limit).await;
    (StatusCode::OK, Json(serde_json::json!({"symbol": sym, "interval": interval, "candles": candles, "count": candles.len()}))).into_response()
}

// ── Order Amend ────────────────────────────────────────────

async fn amend_order(State(s): State<AppState>, Path(id): Path<Uuid>, Json(r): Json<AmendReq>) -> impl IntoResponse {
    match s.engine.amend_order(id, r.price, r.quantity).await {
        Ok(o) => {
            let _ = s.ws_tx.send(WsEvent::OrderUpdate {
                order_id: o.id.to_string(),
                status: "Amended".into(),
                filled_quantity: o.filled_quantity,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
            (StatusCode::OK, Json(serde_json::json!(o))).into_response()
        }
        Err(e) => { (e.to_http_status(), Json(e.binance_json())).into_response() }
    }
}

// ── Portfolio ──────────────────────────────────────────────

async fn get_portfolio(State(s): State<AppState>, Extension(auth): Extension<crate::auth::middleware::AuthUser>, Path(uid): Path<String>) -> impl IntoResponse {
    if auth.user_id != uid {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"User ID mismatch"}))).into_response();
    }
    let portfolio = s.engine.get_portfolio(&uid).await;
    (StatusCode::OK, Json(portfolio)).into_response()
}

// ── API Key Generation ─────────────────────────────────────

async fn generate_api_key(State(s): State<AppState>, Json(r): Json<ApiKeyReq>) -> impl IntoResponse {
    let key = crate::auth::generate_api_key(&r.user_id);
    s.api_keys.insert(key.api_key.clone(), key.clone());
    tracing::info!("Generated API key for user: {}", r.user_id);
    (StatusCode::OK, Json(serde_json::json!({
        "user_id": r.user_id,
        "api_key": key.api_key,
        "secret_key": key.secret_key,
        "message": "Store your secret key securely. It will not be shown again."
    }))).into_response()
}

// ── Config ─────────────────────────────────────────────────

async fn get_config(State(_s): State<AppState>) -> impl IntoResponse {
    let markets = crate::types::default_markets();
    (StatusCode::OK, Json(serde_json::json!({
        "markets": markets,
        "count": markets.len(),
        "default_markets": ["BTC/USD", "ETH/USD", "SOL/USD", "BTC/ETH", "ADA/USD"]
    }))).into_response()
}

// ── Types ──────────────────────────────────────────────────

#[derive(Deserialize)] struct OBQ { symbol: Option<String>, depth: Option<usize> }
#[derive(Deserialize)] struct MQ { symbol: Option<String> }
#[derive(Deserialize)] struct TradeQ { symbol: Option<String>, limit: Option<usize> }
#[derive(Deserialize)] struct DepR { user_id: String, asset: String, amount: f64 }
#[derive(Deserialize)] struct PageQ { limit: Option<usize>, offset: Option<usize> }
#[derive(Serialize)] struct BalR { asset: String, available: f64, locked: f64, total: f64 }
#[derive(Deserialize)] struct CandleQ { symbol: Option<String>, interval: Option<String>, limit: Option<usize> }
#[derive(Deserialize)] struct AmendReq { price: Option<f64>, quantity: Option<f64> }
#[derive(Deserialize)] struct ApiKeyReq { user_id: String }
