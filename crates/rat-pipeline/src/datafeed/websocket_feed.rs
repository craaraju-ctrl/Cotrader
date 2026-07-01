//! WebSocket Feed — Real-time price streaming from Binance.

use tokio::sync::broadcast;
use futures_util::StreamExt;

pub struct WebSocketFeed {
    symbols: Vec<String>,
    tx: broadcast::Sender<PriceUpdate>,
}

#[derive(Debug, Clone)]
pub struct PriceUpdate {
    pub symbol: String,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
}

impl WebSocketFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { symbols, tx }
    }

    pub async fn start(&self) {
        for symbol in &self.symbols {
            let symbol = symbol.clone();
            let tx = self.tx.clone();
            tokio::spawn(async move {
                Self::stream_symbol(&symbol, tx).await;
            });
        }
    }

    async fn stream_symbol(symbol: &str, tx: broadcast::Sender<PriceUpdate>) {
        let url = format!("wss://stream.binance.com:9443/ws/{}usdt@trade", symbol.to_lowercase());
        let _ = symbol;

        loop {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((mut ws, _)) => {
                    while let Some(msg) = ws.next().await {
                        if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let (Some(price_str), Some(qty_str)) = (
                                    json.get("p").and_then(|p| p.as_str()),
                                    json.get("q").and_then(|q| q.as_str()),
                                ) {
                                    if let (Ok(price), Ok(qty)) = (price_str.parse::<f64>(), qty_str.parse::<f64>()) {
                                        let _ = tx.send(PriceUpdate {
                                            symbol: symbol.to_string(),
                                            price,
                                            volume: qty,
                                            timestamp: chrono::Utc::now().timestamp_millis(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PriceUpdate> {
        self.tx.subscribe()
    }
}
