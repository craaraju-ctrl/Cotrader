use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::auth::ApiKeyPair;
use crate::engine::ExchangeEngine;

use super::types::*;

// ── RAT Broadcast Channel ─────────────────────────────────

/// Create a dedicated broadcast channel for RAT agent events.
/// Separate from the public WsEvent channel to avoid leaking agent-level data.
pub fn create_rat_channel() -> broadcast::Sender<RatEvent> {
    let (tx, _) = broadcast::channel(2048);
    tx
}

/// Sequence counter for ordering RAT events
static RAT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn next_seq() -> u64 {
    RAT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

// ── Broadcast Helpers ─────────────────────────────────────

/// Broadcast an orderbook snapshot to all connected RAT agents
pub async fn broadcast_rat_orderbook(
    tx: &broadcast::Sender<RatEvent>,
    symbol: &str,
    engine: &ExchangeEngine,
) {
    match engine.get_orderbook(symbol, 15).await {
        Ok(snap) => {
            let bids: Vec<[f64; 2]> = snap.bids.iter().map(|l| [l.price, l.quantity]).collect();
            let asks: Vec<[f64; 2]> = snap.asks.iter().map(|l| [l.price, l.quantity]).collect();
            let _ = tx.send(RatEvent::OrderbookSnapshot(RatOrderbookSnapshot {
                symbol: symbol.to_string(),
                bids,
                asks,
                timestamp: chrono::Utc::now(),
                sequence: next_seq(),
            }));
        }
        Err(e) => {
            tracing::warn!("[RAT] Failed to broadcast orderbook for {}: {}", symbol, e);
        }
    }
}

/// Broadcast a balance update for a specific user/asset
pub async fn broadcast_rat_balance(
    tx: &broadcast::Sender<RatEvent>,
    user_id: &str,
    asset: &str,
    available: f64,
    locked: f64,
) {
    let total = available + locked;
    let _ = tx.send(RatEvent::BalanceUpdate(RatBalanceUpdate {
        user_id: user_id.to_string(),
        asset: asset.to_string(),
        available,
        locked,
        total,
        timestamp: chrono::Utc::now(),
    }));
}

/// Broadcast a funding rate tick
pub async fn broadcast_rat_funding(
    tx: &broadcast::Sender<RatEvent>,
    symbol: &str,
    rate: f64,
    mark_price: f64,
    next_funding_time: chrono::DateTime<chrono::Utc>,
) {
    let _ = tx.send(RatEvent::FundingTick(RatFundingTick {
        symbol: symbol.to_string(),
        funding_rate: rate,
        mark_price,
        next_funding_time,
        timestamp: chrono::Utc::now(),
    }));
}

/// Broadcast a trade execution
pub async fn broadcast_rat_trade(
    tx: &broadcast::Sender<RatEvent>,
    trade: &crate::types::Trade,
) {
    let _ = tx.send(RatEvent::TradeExecution(RatTradeExecution {
        trade_id: trade.id,
        symbol: trade.symbol.clone(),
        price: trade.price,
        quantity: trade.quantity,
        total: trade.total,
        buyer_id: trade.buyer_id.clone(),
        seller_id: trade.seller_id.clone(),
        taker_side: format!("{:?}", trade.taker_side),
        timestamp: trade.timestamp,
    }));
}

/// Broadcast an agent decision log
pub fn broadcast_rat_decision(
    tx: &broadcast::Sender<RatEvent>,
    decision: RatAgentDecision,
) {
    let _ = tx.send(RatEvent::AgentDecision(decision));
}

/// Broadcast a diagnostic message
pub fn broadcast_rat_diagnostic(
    tx: &broadcast::Sender<RatEvent>,
    level: &str,
    message: &str,
    module: &str,
) {
    let _ = tx.send(RatEvent::Diagnostic(RatDiagnostic {
        level: level.to_string(),
        message: message.to_string(),
        module: module.to_string(),
        timestamp: chrono::Utc::now(),
    }));
}

// ── Query params ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RatWsQuery {
    pub api_key: String,
    pub signature: String,
    pub nonce: String,
    pub symbols: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RatRestQuery {
    pub api_key: String,
    pub signature: String,
    pub nonce: String,
    pub symbol: Option<String>,
}

// ── HMAC verification for RAT WS connections ─────────────

/// Verify HMAC authentication for a RAT WebSocket upgrade request.
fn verify_rat_auth(
    query: &RatWsQuery,
    api_keys: &DashMap<String, ApiKeyPair>,
) -> Result<String, &'static str> {
    // Validate nonce timestamp (within 5 minutes)
    let ts: i64 = query.nonce.parse().map_err(|_| "Invalid nonce")?;
    if !crate::auth::validate_timestamp(ts) {
        return Err("Nonce expired");
    }

    // Look up API key
    let entry = api_keys.get(&query.api_key).ok_or("Invalid API key")?;
    let pair = entry.value();
    let user_id = pair.user_id.clone();
    let secret_key = pair.secret_key.clone();
    drop(entry);

    // Reconstruct message: "WS/rat/stream" + NONCE
    // We use a fixed path for WS auth to match the HMAC scheme
    let message = format!("WS/api/v1/rat/stream{}", query.nonce);

    if !crate::auth::verify_signature(&secret_key, &message, &query.signature) {
        return Err("Invalid signature");
    }

    Ok(user_id)
}

/// Verify HMAC for REST snapshot endpoint
fn verify_rat_rest_auth(
    query: &RatRestQuery,
    api_keys: &DashMap<String, ApiKeyPair>,
) -> Result<String, &'static str> {
    let ts: i64 = query.nonce.parse().map_err(|_| "Invalid nonce")?;
    if !crate::auth::validate_timestamp(ts) {
        return Err("Nonce expired");
    }

    let entry = api_keys.get(&query.api_key).ok_or("Invalid API key")?;
    let pair = entry.value();
    let user_id = pair.user_id.clone();
    let secret_key = pair.secret_key.clone();
    drop(entry);

    let message = format!("GET/api/v1/rat/snapshot{}", query.nonce);

    if !crate::auth::verify_signature(&secret_key, &message, &query.signature) {
        return Err("Invalid signature");
    }

    Ok(user_id)
}

// ── WS Handler ────────────────────────────────────────────

/// WebSocket upgrade handler for the RAT agent stream.
/// Authenticated via HMAC query parameters (not headers, since WS upgrade
/// doesn't easily carry custom headers from all WS clients).
pub async fn rat_ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<RatWsQuery>,
    State(state): State<crate::api::AppState>,
) -> impl IntoResponse {
    let user_id = match verify_rat_auth(&params, &state.api_keys) {
        Ok(uid) => uid,
        Err(e) => {
            tracing::warn!("RAT WS auth failed: {}", e);
            return (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }
    };

    let symbols_filter: Vec<String> = params
        .symbols
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    ws.on_upgrade(move |socket| {
        handle_rat_socket(socket, state.rat_tx, state.engine.clone(), symbols_filter, user_id)
    })
    .into_response()
}

/// Background per-socket task that forwards RatEvents from the broadcast channel.
async fn handle_rat_socket(
    socket: WebSocket,
    rat_tx: broadcast::Sender<RatEvent>,
    engine: ExchangeEngine,
    symbols_filter: Vec<String>,
    user_id: String,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = rat_tx.subscribe();

    // 1. Send initial state snapshot on connect
    send_state_snapshot(&mut sender, &engine, &symbols_filter, &user_id).await;

    // 2. Forward broadcast events with symbol filtering
    let mut send_task = tokio::spawn(async move {
        let mut heartbeat_interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tokio::select! {
                Ok(event) = rx.recv() => {
                    if !symbols_filter.is_empty() {
                        let symbol = match &event {
                            RatEvent::OrderbookSnapshot(s) => Some(&s.symbol),
                            RatEvent::FundingTick(f) => Some(&f.symbol),
                            RatEvent::TradeExecution(t) => Some(&t.symbol),
                            RatEvent::PositionChange(p) => Some(&p.symbol),
                            _ => None,
                        };
                        if let Some(sym) = symbol {
                            if !symbols_filter.iter().any(|f| f == sym) {
                                continue;
                            }
                        }
                    }

                    if let Ok(json) = serde_json::to_string(&event) {
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                _ = heartbeat_interval.tick() => {
                    let hb = RatEvent::Heartbeat(RatHeartbeat {
                        timestamp: chrono::Utc::now(),
                        sequence: next_seq(),
                    });
                    if let Ok(json) = serde_json::to_string(&hb) {
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    // 3. Handle incoming commands from the RAT agent
    let rat_tx_clone = rat_tx.clone();
    let engine_clone = engine.clone();
    let uid = user_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(cmd) = serde_json::from_str::<RatCommand>(&text) {
                        handle_rat_command(cmd, &rat_tx_clone, &engine_clone, &uid).await;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    tracing::info!("RAT agent disconnected: {}", user_id);
}

/// Send initial state snapshot to a newly connected RAT agent
async fn send_state_snapshot(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    engine: &ExchangeEngine,
    symbols_filter: &[String],
    _user_id: &str,
) {
    let symbols = if symbols_filter.is_empty() {
        engine.get_symbols().await
    } else {
        symbols_filter.to_vec()
    };

    for sym in &symbols {
        if let Ok(snap) = engine.get_orderbook(sym, 15).await {
            let bids: Vec<[f64; 2]> = snap.bids.iter().map(|l| [l.price, l.quantity]).collect();
            let asks: Vec<[f64; 2]> = snap.asks.iter().map(|l| [l.price, l.quantity]).collect();
            let event = RatEvent::OrderbookSnapshot(RatOrderbookSnapshot {
                symbol: sym.clone(),
                bids,
                asks,
                timestamp: chrono::Utc::now(),
                sequence: next_seq(),
            });
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = sender.send(Message::Text(json)).await;
            }
        }
    }

    // Send diagnostic confirming connection
    let diag = RatEvent::Diagnostic(RatDiagnostic {
        level: "info".into(),
        message: format!(
            "RAT agent connected. Streaming {} symbols.",
            symbols.len()
        ),
        module: "rat::stream".into(),
        timestamp: chrono::Utc::now(),
    });
    if let Ok(json) = serde_json::to_string(&diag) {
        let _ = sender.send(Message::Text(json)).await;
    }
}

/// Process an incoming command from a RAT agent
async fn handle_rat_command(
    cmd: RatCommand,
    rat_tx: &broadcast::Sender<RatEvent>,
    engine: &ExchangeEngine,
    user_id: &str,
) {
    match cmd {
        RatCommand::PlaceOrder(order) => {
            tracing::info!("RAT agent {} placing order on {}", user_id, order.symbol);
            let side = match order.side.to_lowercase().as_str() {
                "buy" => crate::types::Side::Buy,
                _ => crate::types::Side::Sell,
            };
            let order_type = match order.order_type.to_lowercase().as_str() {
                "limit" => crate::types::OrderType::Limit,
                "market" => crate::types::OrderType::Market,
                "stop_loss" | "stoploss" => crate::types::OrderType::StopLoss,
                "stop_limit" | "stoplimit" => crate::types::OrderType::StopLimit,
                "take_profit" | "takeprofit" => crate::types::OrderType::TakeProfit,
                _ => {
                    broadcast_rat_diagnostic(rat_tx, "error", &format!("Unknown order type: {}", order.order_type), "rat::stream");
                    return;
                }
            };

            let tredo_order = match order_type {
                crate::types::OrderType::Limit => {
                    crate::types::Order::new_limit(
                        order.user_id.clone(), order.symbol.clone(), side,
                        order.price.unwrap_or(0.0), order.quantity,
                    )
                }
                crate::types::OrderType::Market => {
                    crate::types::Order::new_market(
                        order.user_id.clone(), order.symbol.clone(), side, order.quantity,
                    )
                }
                crate::types::OrderType::StopLoss => {
                    crate::types::Order::new_stop_loss(
                        order.user_id.clone(), order.symbol.clone(), side,
                        order.trigger_price.unwrap_or(0.0), order.quantity,
                    )
                }
                crate::types::OrderType::StopLimit => {
                    crate::types::Order::new_stop_limit(
                        order.user_id.clone(), order.symbol.clone(), side,
                        order.trigger_price.unwrap_or(0.0), order.price.unwrap_or(0.0), order.quantity,
                    )
                }
                crate::types::OrderType::TakeProfit => {
                    crate::types::Order::new_take_profit(
                        order.user_id.clone(), order.symbol.clone(), side,
                        order.trigger_price.unwrap_or(0.0), order.quantity,
                    )
                }
                _ => {
                    broadcast_rat_diagnostic(rat_tx, "error", "Order type not supported via RAT", "rat::stream");
                    return;
                }
            };

            match engine.place_order(tredo_order).await {
                Ok(resp) => {
                    broadcast_rat_diagnostic(
                        rat_tx, "info",
                        &format!("Order placed: {} (status: {:?})", resp.order_id, resp.status),
                        "rat::stream",
                    );
                    // Broadcast resulting trades
                    for trade in &resp.trades {
                        broadcast_rat_trade(rat_tx, trade).await;
                    }
                }
                Err(e) => {
                    broadcast_rat_diagnostic(
                        rat_tx, "error",
                        &format!("Order failed: {}", e),
                        "rat::stream",
                    );
                }
            }
        }
        RatCommand::CancelOrder { order_id, symbol } => {
            match engine.cancel_order(&symbol, order_id).await {
                Ok(o) => {
                    broadcast_rat_diagnostic(
                        rat_tx, "info",
                        &format!("Order cancelled: {}", o.id),
                        "rat::stream",
                    );
                }
                Err(e) => {
                    broadcast_rat_diagnostic(
                        rat_tx, "error",
                        &format!("Cancel failed: {}", e),
                        "rat::stream",
                    );
                }
            }
        }
        RatCommand::RequestSnapshot { symbols } => {
            let syms = match symbols {
                Some(s) => s,
                None => engine.get_symbols().await,
            };
            for sym in syms {
                broadcast_rat_orderbook(rat_tx, &sym, engine).await;
            }
        }
        RatCommand::UpdateConfig { .. } => {
            broadcast_rat_diagnostic(rat_tx, "info", "Config update acknowledged", "rat::stream");
        }
        RatCommand::Pong => {
            // Acknowledge silently
        }
    }
}

// ── REST Snapshot Endpoint ────────────────────────────────

/// REST endpoint for RAT agents to pull a full system snapshot
/// without maintaining a persistent WebSocket connection.
pub async fn rat_rest_snapshot(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Query(query): axum::extract::Query<RatRestQuery>,
) -> impl axum::response::IntoResponse {
    let _user_id = match verify_rat_rest_auth(&query, &state.api_keys) {
        Ok(uid) => uid,
        Err(e) => {
            return (axum::http::StatusCode::UNAUTHORIZED, axum::Json(serde_json::json!({
                "error": format!("Auth failed: {}", e)
            }))).into_response();
        }
    };

    let engine = &state.engine;
    let symbols = if let Some(ref sym) = query.symbol {
        vec![sym.clone()]
    } else {
        engine.get_symbols().await
    };

    let mut snapshots = Vec::new();
    for sym in &symbols {
        if let Ok(snap) = engine.get_orderbook(sym, 15).await {
            let bids: Vec<[f64; 2]> = snap.bids.iter().map(|l| [l.price, l.quantity]).collect();
            let asks: Vec<[f64; 2]> = snap.asks.iter().map(|l| [l.price, l.quantity]).collect();
            snapshots.push(serde_json::json!({
                "symbol": sym,
                "bids": bids,
                "asks": asks,
                "timestamp": chrono::Utc::now(),
            }));
        }
    }

    let mut funding_rates = Vec::new();
    for sym in &symbols {
        let rate = engine.get_funding_rate(sym).await;
        funding_rates.push(serde_json::json!({
            "symbol": sym,
            "funding_rate": rate.funding_rate,
            "next_funding_time": rate.next_funding_time,
        }));
    }

    (axum::http::StatusCode::OK, axum::Json(serde_json::json!({
        "snapshots": snapshots,
        "funding_rates": funding_rates,
        "timestamp": chrono::Utc::now(),
        "sequence": next_seq(),
    }))).into_response()
}
