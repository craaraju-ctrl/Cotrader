//! Binance Broker Adapter

pub struct BinanceBroker;

impl BinanceBroker {
    pub fn name() -> &'static str { "BinanceBroker" }
    pub fn connect(&self) -> bool { true }
}
