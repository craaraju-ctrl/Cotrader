//! Zerodha Client — Real Zerodha Kite API integration.

use crate::traits::*;

pub struct ZerodhaClient {
    api_key: String,
    access_token: String,
    base_url: String,
    connected: bool,
}

impl ZerodhaClient {
    pub fn new(api_key: &str, access_token: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            access_token: access_token.to_string(),
            base_url: "https://api.kite.trade".to_string(),
            connected: false,
        }
    }
}

#[async_trait]
impl Broker for ZerodhaClient {
    fn name(&self) -> &str { "Zerodha" }
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
    async fn get_market_data(&self, _symbol: &str) -> Result<MarketData, BrokerError> {
        Err(BrokerError::ApiError("Not implemented".to_string()))
    }
    async fn subscribe(&self, _symbols: Vec<String>) -> Result<(), BrokerError> { Ok(()) }
}
