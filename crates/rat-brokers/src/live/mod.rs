//! Live Broker Connections — Real exchange API integration.

pub mod binance_client;
pub mod zerodha_client;

pub use binance_client::BinanceClient;
pub use zerodha_client::ZerodhaClient;

use crate::traits::Broker;

pub struct LiveBrokerRouter {
    brokers: Vec<Box<dyn Broker>>,
    active_broker: usize,
}

impl LiveBrokerRouter {
    pub fn new() -> Self {
        Self {
            brokers: Vec::new(),
            active_broker: 0,
        }
    }

    pub fn add_broker(&mut self, broker: Box<dyn Broker>) {
        self.brokers.push(broker);
    }

    pub async fn select_best_broker(&mut self, symbol: &str) -> &str {
        let _ = symbol;
        if let Some(broker) = self.brokers.get(self.active_broker) {
            broker.name()
        } else {
            "paper"
        }
    }
}
