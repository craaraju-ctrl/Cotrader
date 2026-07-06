//! Broker plugin framework — plug-and-play broker connections.

pub mod plugin_registry;
pub mod sandbox;

pub use plugin_registry::{BrokerConfig, BrokerHandle, BrokerPluginManager, PluginRegistry};
pub use sandbox::BrokerSandbox;
