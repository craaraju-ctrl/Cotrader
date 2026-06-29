use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use axum::extract::Path as AxumPath;

use super::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    #[serde(rename = "Trade")]
    Trade {
        symbol: String,
        price: f64,
        quantity: f64,
        buyer_id: String,
        seller_id: String,
        timestamp: String,
    },
    #[serde(rename = "OrderBookUpdate")]
    OrderBookUpdate {
        symbol: String,
        /// Best bid price scaled by 10,000 as u64 (convert: f64_price = u64_price / 10000.0)
        best_bid: Option<u64>,
        /// Best ask price scaled by 10,000 as u64 (convert: f64_price = u64_price / 10000.0)
        best_ask: Option<u64>,
        /// Spread as u64 (best_ask_scaled - best_bid_scaled)
        spread: Option<u64>,
        timestamp: String,
    },
    #[serde(rename = "OrderUpdate")]
    OrderUpdate {
        order_id: String,
        status: String,
        filled_quantity: f64,
        timestamp: String,
    },
    /// Full order book depth snapshot/update (like Binance's <symbol>@depth)
    #[serde(rename = "DepthUpdate")]
    DepthUpdate {
        symbol: String,
        /// Bids as [price, quantity] arrays
        bids: Vec<[f64; 2]>,
        /// Asks as [price, quantity] arrays
        asks: Vec<[f64; 2]>,
        timestamp: String,
    },
    #[serde(rename = "pong")]
    Pong,
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
    pub symbols: Option<String>,
}

pub fn create_broadcast_channel() -> broadcast::Sender<WsEvent> {
    let (tx, _) = broadcast::channel(1024);
    tx
}

/// WebSocket depth stream handler.
/// On connect: sends full depth snapshot immediately.
/// Then streams DepthUpdate events from the broadcast channel.
pub async fn ws_depth_handler(
    ws: WebSocketUpgrade,
    AxumPath(symbol): AxumPath<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let tx = state.ws_tx.clone();
    let engine = state.engine.clone();
    let sym = symbol.clone();

    ws.on_upgrade(move |socket| handle_depth_socket(socket, tx, engine, sym))
}

async fn handle_depth_socket(
    socket: WebSocket,
    tx: broadcast::Sender<WsEvent>,
    engine: crate::engine::ExchangeEngine,
    symbol: String,
) {
    let (mut sender, mut receiver) = socket.split();

    // 1. Send initial full depth snapshot
    if let Ok(snap) = engine.get_orderbook(&symbol, 15).await {
        let bids: Vec<[f64; 2]> = snap.bids.iter().map(|l| [l.price, l.quantity]).collect();
        let asks: Vec<[f64; 2]> = snap.asks.iter().map(|l| [l.price, l.quantity]).collect();
        let snapshot = WsEvent::DepthUpdate {
            symbol: symbol.clone(),
            bids,
            asks,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        if let Ok(json) = serde_json::to_string(&snapshot) {
            let _ = sender.send(Message::Text(json)).await;
        }
    } else {
        let _ = sender.send(Message::Text(
            serde_json::json!({"type":"error","msg":"Symbol not found"}).to_string()
        )).await;
        return;
    }

    // 2. Subscribe to broadcast and forward DepthUpdate events for this symbol
    let mut rx = tx.subscribe();
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match &event {
                WsEvent::DepthUpdate { symbol: sym, .. } if sym == &symbol => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let tx = state.ws_tx.clone();
    let symbols_filter: Vec<String> = params
        .symbols
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    ws.on_upgrade(move |socket| handle_socket(socket, tx, symbols_filter))
}

async fn handle_socket(
    socket: WebSocket,
    tx: broadcast::Sender<WsEvent>,
    symbols_filter: Vec<String>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = tx.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if !symbols_filter.is_empty() {
                let symbol = match &event {
                    WsEvent::Trade { symbol, .. } => symbol,
                    WsEvent::OrderBookUpdate { symbol, .. } => symbol,
                    _ => continue,
                };
                if !symbols_filter.iter().any(|f| f == symbol) {
                    continue;
                }
            }

            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                        if parsed.get("type").and_then(|v| v.as_str()) == Some("ping") {
                            let _ = tx.send(WsEvent::Pong);
                        }
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
}

pub async fn broadcast_trade(tx: &broadcast::Sender<WsEvent>, trade: &crate::types::Trade) {
    let _ = tx.send(WsEvent::Trade {
        symbol: trade.symbol.clone(),
        price: trade.price,
        quantity: trade.quantity,
        buyer_id: trade.buyer_id.clone(),
        seller_id: trade.seller_id.clone(),
        timestamp: trade.timestamp.to_rfc3339(),
    });
}

/// Scale an f64 price to the u64 representation (price * 10000)
pub fn scale_price(price: f64) -> u64 {
    (price * 10_000.0).round() as u64
}

/// Scale an Option<f64> price to Option<u64>
pub fn scale_price_opt(price: Option<f64>) -> Option<u64> {
    price.map(scale_price)
}

/// Broadcast full depth update (top 15 bids + asks).
pub async fn broadcast_depth(
    tx: &broadcast::Sender<WsEvent>,
    engine: &crate::engine::ExchangeEngine,
    symbol: &str,
) {
    if let Ok(snap) = engine.get_orderbook(symbol, 15).await {
        let bids: Vec<[f64; 2]> = snap.bids.iter().map(|l| [l.price, l.quantity]).collect();
        let asks: Vec<[f64; 2]> = snap.asks.iter().map(|l| [l.price, l.quantity]).collect();
        let _ = tx.send(WsEvent::DepthUpdate {
            symbol: symbol.to_string(),
            bids,
            asks,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }
}

/// Broadcast an orderbook update via WebSocket with u64 scaled prices
pub async fn broadcast_orderbook_update(
    tx: &broadcast::Sender<WsEvent>,
    symbol: &str,
    best_bid: Option<u64>,
    best_ask: Option<u64>,
    spread: Option<u64>,
) {
    let _ = tx.send(WsEvent::OrderBookUpdate {
        symbol: symbol.to_string(),
        best_bid,
        best_ask,
        spread,
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
}
