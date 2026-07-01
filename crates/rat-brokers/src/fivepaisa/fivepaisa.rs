//! Fivepaisa Broker Adapter

pub struct FivepaisaBroker;

impl FivepaisaBroker {
    pub fn name() -> &'static str { "FivepaisaBroker" }
    pub fn connect(&self) -> bool { true }
}
