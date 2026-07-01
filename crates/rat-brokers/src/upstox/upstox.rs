//! Upstox Broker Adapter

pub struct UpstoxBroker;

impl UpstoxBroker {
    pub fn name() -> &'static str { "UpstoxBroker" }
    pub fn connect(&self) -> bool { true }
}
