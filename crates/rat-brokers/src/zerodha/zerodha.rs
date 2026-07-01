//! Zerodha Broker Adapter

pub struct ZerodhaBroker;

impl ZerodhaBroker {
    pub fn name() -> &'static str { "ZerodhaBroker" }
    pub fn connect(&self) -> bool { true }
}
