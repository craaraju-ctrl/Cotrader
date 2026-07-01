//! Binance Client — Real Binance API integration.

use crate::traits::*;
use reqwest;

pub struct BinanceClient {
    api_key: String,
    api_secret: String,
    base_url: String,
    client: reqwest::Client,
    connected: bool,
}

impl BinanceClient {
    pub fn new(api_key: &str, api_secret: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
            base_url: "https://api.binance.com".to_string(),
            client: reqwest::Client::new(),
            connected: false,
        }
    }

    pub async fn get_ticker(&self, symbol: &str) -> Result<f64, BrokerError> {
        let url = format!("{}/api/v3/ticker/price?symbol={}USDT", self.base_url, symbol);
        let resp = self.client.get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| BrokerError::ApiError(e.to_string()))?;

        let json: serde_json::Value = resp.json().await
            .map_err(|e| BrokerError::ApiError(e.to_string()))?;

        json.get("price")
            .and_then(|p| p.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or(BrokerError::ApiError("Invalid price".to_string()))
    }

    pub async fn get_klines(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Kline>, BrokerError> {
        let url = format!(
            "{}/api/v3/klines?symbol={}USDT&interval={}&limit={}",
            self.base_url, symbol, interval, limit
        );
        let resp = self.client.get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| BrokerError::ApiError(e.to_string()))?;

        let json: Vec<Vec<serde_json::Value>> = resp.json().await
            .map_err(|e| BrokerError::ApiError(e.to_string()))?;

        let klines = json.iter().filter_map(|k| {
            if k.len() >= 6 {
                Some(Kline {
                    open_time: k[0].as_i64().unwrap_or(0),
                    open: k[1].as_str()?.parse().ok()?,
                    high: k[2].as_str()?.parse().ok()?,
                    low: k[3].as_str()?.parse().ok()?,
                    close: k[4].as_str()?.parse().ok()?,
                    volume: k[5].as_str()?.parse().ok()?,
                })
            } else {
                None
            }
        }).collect();

        Ok(klines)
    }
}

#[derive(Debug, Clone)]
pub struct Kline {
    pub open_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[async_trait]
impl Broker for BinanceClient {
    fn name(&self) -> &str { "Binance" }
    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.connected = true;
        Ok(())
    }
    async fn disconnect(&mut self) -> Result<(), BrokerError> {
        self.connected = false;
        Ok(())
    }
    fn is_connected(&self) -> bool { self.connected }
    async fn place_order(&self, _order: NewOrder) -> Result<OrderId, BrokerError> {
        Err(BrokerError::ApiError("Not implemented".to_string()))
    }
    async fn cancel_order(&self, _order_id: &OrderId) -> Result<(), BrokerError> {
        Err(BrokerError::ApiError("Not implemented".to_string()))
    }
    async fn get_open_orders(&self, _symbol: &str) -> Result<Vec<Order>, BrokerError> { Ok(vec![]) }
    async fn get_positions(&self) -> Result<Vec<Position>, BrokerError> { Ok(vec![]) }
    async fn get_balance(&self) -> Result<Balance, BrokerError> {
        Ok(Balance { total: 0.0, available: 0.0, margin_used: 0.0, unrealized_pnl: 0.0 })
    }
    async fn get_market_data(&self, symbol: &str) -> Result<MarketData, BrokerError> {
        let price = self.get_ticker(symbol).await?;
        Ok(MarketData {
            symbol: symbol.to_string(),
            bid: price * 0.9999,
            ask: price * 1.0001,
            last: price,
            volume: 0.0,
            timestamp: chrono::Utc::now(),
        })
    }
    async fn subscribe(&self, _symbols: Vec<String>) -> Result<(), BrokerError> { Ok(()) }
}
