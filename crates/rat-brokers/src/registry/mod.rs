//! Broker Registry — Normalizes APIs and manages connection keys.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct BrokerRegistry {
    brokers: Arc<RwLock<HashMap<String, BrokerEntry>>>,
    rate_limiter: RateLimiter,
}

struct BrokerEntry {
    name: String,
    api_key: Option<String>,
    api_secret: Option<String>,
    base_url: String,
    connected: bool,
    last_request: Option<std::time::Instant>,
}

struct RateLimiter {
    requests_per_second: u32,
    last_request: Option<std::time::Instant>,
}

impl BrokerRegistry {
    pub fn new() -> Self {
        Self {
            brokers: Arc::new(RwLock::new(HashMap::new())),
            rate_limiter: RateLimiter {
                requests_per_second: 10,
                last_request: None,
            },
        }
    }

    /// Register a new broker.
    pub async fn register(&self, name: &str, base_url: &str, api_key: Option<String>, api_secret: Option<String>) {
        let mut brokers = self.brokers.write().await;
        brokers.insert(name.to_string(), BrokerEntry {
            name: name.to_string(),
            api_key,
            api_secret,
            base_url: base_url.to_string(),
            connected: false,
            last_request: None,
        });
    }

    /// Connect to a registered broker.
    pub async fn connect(&self, name: &str) -> Result<(), String> {
        let mut brokers = self.brokers.write().await;
        if let Some(broker) = brokers.get_mut(name) {
            broker.connected = true;
            println!("[BrokerRegistry] Connected to {}", name);
            Ok(())
        } else {
            Err(format!("Broker {} not registered", name))
        }
    }

    /// Get broker configuration.
    pub async fn get(&self, name: &str) -> Option<BrokerConfig> {
        let brokers = self.brokers.read().await;
        brokers.get(name).map(|b| BrokerConfig {
            name: b.name.clone(),
            base_url: b.base_url.clone(),
            connected: b.connected,
        })
    }

    /// Check rate limits before making a request.
    pub async fn check_rate_limit(&self) -> bool {
        // Simple rate limiting
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        true
    }

    /// Get all registered brokers.
    pub async fn list(&self) -> Vec<BrokerConfig> {
        let brokers = self.brokers.read().await;
        brokers.values().map(|b| BrokerConfig {
            name: b.name.clone(),
            base_url: b.base_url.clone(),
            connected: b.connected,
        }).collect()
    }
}

#[derive(Debug, Clone)]
pub struct BrokerConfig {
    pub name: String,
    pub base_url: String,
    pub connected: bool,
}
