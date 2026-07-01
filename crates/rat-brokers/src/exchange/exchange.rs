//! Exchange Broker Adapter

pub struct ExchangeBroker;

impl ExchangeBroker {
    pub fn name() -> &'static str { "ExchangeBroker" }
    pub fn connect(&self) -> bool { true }
}
