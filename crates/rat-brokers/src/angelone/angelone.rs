//! Angelone Broker Adapter

pub struct AngeloneBroker;

impl AngeloneBroker {
    pub fn name() -> &'static str { "AngeloneBroker" }
    pub fn connect(&self) -> bool { true }
}
