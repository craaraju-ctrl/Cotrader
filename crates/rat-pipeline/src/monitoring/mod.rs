//! Monitoring Dashboard — Web-based metrics view.

pub mod health;
pub mod metrics_server;

pub use health::HealthChecker;
pub use metrics_server::MetricsServer;
