//! Alpaca Broker Adapter

pub struct AlpacaBroker;

impl AlpacaBroker {
    pub fn name() -> &'static str { "AlpacaBroker" }
    pub fn connect(&self) -> bool { true }
}
