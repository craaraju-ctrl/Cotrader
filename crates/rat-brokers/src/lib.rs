//! rat-brokers — Multi-broker abstraction layer.

pub mod traits;
pub mod registry;
pub mod sandbox;
pub mod engine;
pub mod alpaca;
pub mod angelone;
pub mod binance;
pub mod exchange;
pub mod fivepaisa;
pub mod upstox;
pub mod zerodha;

pub use traits::{Broker, NewOrder, OrderId, OrderSide, OrderType, OrderStatus, Order, Position, Balance, MarketData, BrokerError};
pub use traits::paper::PaperBroker;
pub use registry::BrokerRegistry;
pub use sandbox::Sandbox;
pub use engine::Engine;
