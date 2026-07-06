//! # Memory Client SDK — Universal Agent Adapter
//!
//! A resilient HTTP client for the Memory API with automatic retry, exponential backoff,
//! and circuit breaker protection. Any agent can use this to store, search, and manage memories.
//!
//! ## Quick Start
//!
//! ```no_run
//! # async fn doc() {
//! use agentic_memory::client::MemoryClient;
//!
//! let client = MemoryClient::new("http://localhost:3111")
//!     .with_retry(3, 500)            // 3 retries, 500ms base delay
//!     .with_circuit_breaker(5, 30);  // 5 failures → open, 30s reset
//!
//! // Insert a memory
//! let id = client.insert("I learned that Rust uses ownership", "fact", "semantic", 0.8).await.unwrap();
//!
//! // Search memories
//! let results = client.search("Rust ownership", 5).await.unwrap();
//!
//! // Get a specific memory
//! let record = client.get(&id).await.unwrap();
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::resilience::{CircuitBreaker, CircuitState};

/// A resilient HTTP client for the Memory API with automatic retry and circuit breaker.
pub struct MemoryClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    max_retries: u32,
    base_delay_ms: u64,
    circuit_breaker: CircuitBreaker,
}

/// A search result returned by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSearchResult {
    pub id: String,
    pub content: String,
    pub content_type: String,
    pub tier: String,
    pub score: f64,
    pub method: String,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Record details returned by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecord {
    pub id: String,
    pub content: String,
    pub content_type: String,
    pub tier: String,
    pub importance: f64,
    pub access_count: u64,
    pub metadata: std::collections::HashMap<String, String>,
    pub timestamp: String,
}

/// Health status returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub total_records: u64,
    pub graph_edges: u64,
}

/// Stats returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_records: u64,
    pub total_with_embeddings: u64,
    pub storage_bytes: u64,
}

/// Check if an error is retryable (connection errors, 5xx responses).
fn is_retryable_error(err: &str) -> bool {
    if err.contains("connection refused")
        || err.contains("connection timed out")
        || err.contains("dns error")
        || err.contains("broken pipe")
        || err.contains("eof")
    {
        return true;
    }
    if let Some(status_str) = err.split("status=").nth(1) {
        if let Some(code_str) = status_str.split_whitespace().next() {
            if let Ok(code) = code_str.parse::<u16>() {
                return code >= 500;
            }
        }
    }
    false
}

impl MemoryClient {
    /// Create a new client pointing to the Memory API server.
    /// Default circuit breaker: 5 failures → open, 30s reset timeout.
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: None,
            max_retries: 3,
            base_delay_ms: 500,
            circuit_breaker: CircuitBreaker::new(5, 30),
        }
    }

    /// Set an API key for authenticated requests.
    pub fn with_api_key(mut self, api_key: &str) -> Self {
        self.api_key = Some(api_key.to_string());
        self
    }

    /// Configure retry behavior. Default: 3 retries, 500ms base delay.
    pub fn with_retry(mut self, max_retries: u32, base_delay_ms: u64) -> Self {
        self.max_retries = max_retries;
        self.base_delay_ms = base_delay_ms;
        self
    }

    /// Configure circuit breaker. Default: 5 failures → open, 30s reset.
    /// When open, all operations fail immediately without retry.
    /// After the reset timeout, one probe request is attempted (half-open).
    pub fn with_circuit_breaker(mut self, failure_threshold: u32, reset_timeout_secs: u64) -> Self {
        self.circuit_breaker = CircuitBreaker::new(failure_threshold, reset_timeout_secs);
        self
    }

    /// Create from environment variables (MEMORY_API_URL, MEMORY_API_KEY).
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("MEMORY_API_URL").unwrap_or_else(|_| "http://localhost:3111".to_string());
        let api_key = std::env::var("MEMORY_API_KEY").ok();
        let mut client = Self::new(&base_url);
        if let Some(key) = api_key {
            client = client.with_api_key(&key);
        }
        client
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, self.url(path));
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req
    }

    /// Get the current circuit breaker state as a string.
    pub fn circuit_state(&self) -> &'static str {
        match self.circuit_breaker.get_state() {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        }
    }

    /// Execute an async operation with retry, exponential backoff, and circuit breaker.
    async fn execute_with_retry<F, Fut, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T, String>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        if !self.circuit_breaker.can_execute() {
            tracing::warn!(
                "Memory {} blocked — circuit breaker OPEN, failing fast",
                operation_name
            );
            return Err(format!(
                "Memory service unavailable (circuit breaker open). Operation: {}",
                operation_name
            ));
        }

        let mut last_err = String::new();

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(result) => {
                    self.circuit_breaker.on_success();
                    if attempt > 0 {
                        tracing::info!(
                            "Memory {} succeeded after {} retries",
                            operation_name,
                            attempt
                        );
                    }
                    return Ok(result);
                }
                Err(e) if attempt < self.max_retries && is_retryable_error(&e) => {
                    let delay = self.base_delay_ms * 2u64.pow(attempt);
                    tracing::warn!(
                        "Memory {} failed (attempt {}/{}), retrying in {}ms: {}",
                        operation_name,
                        attempt + 1,
                        self.max_retries + 1,
                        delay,
                        e
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    last_err = e;
                }
                Err(e) => {
                    self.circuit_breaker.on_failure();
                    return Err(e);
                }
            }
        }

        self.circuit_breaker.on_failure();
        Err(format!(
            "Memory {} failed after {} retries: {}",
            operation_name,
            self.max_retries + 1,
            last_err
        ))
    }

    /// Check if the memory service is reachable.
    pub async fn is_available(&self) -> bool {
        self.health().await.is_ok()
    }

    // ── Records CRUD ─────────────────────────────────────────────────────

    pub async fn insert(
        &self,
        content: &str,
        content_type: &str,
        tier: &str,
        importance: f64,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "id": uuid::Uuid::now_v7().to_string(),
            "content": content,
            "content_type": content_type,
            "tier": tier,
            "importance": importance,
        });

        self.execute_with_retry("insert", || {
            let body = body.clone();
            let client = &self;
            async move {
                let resp: serde_json::Value = client
                    .build_request(reqwest::Method::POST, "/records")
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;

                resp["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| format!("No id in response: {}", resp))
            }
        })
        .await
    }

    pub async fn insert_with_id(
        &self,
        id: &str,
        content: &str,
        content_type: &str,
        tier: &str,
        importance: f64,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "id": id,
            "content": content,
            "content_type": content_type,
            "tier": tier,
            "importance": importance,
        });

        self.execute_with_retry("insert_with_id", || {
            let body = body.clone();
            let client = &self;
            async move {
                client
                    .build_request(reqwest::Method::POST, "/records")
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?;
                Ok(id.to_string())
            }
        })
        .await
    }

    pub async fn get(&self, id: &str) -> Result<ClientRecord, String> {
        let id = id.to_string();
        self.execute_with_retry("get", || {
            let id = id.clone();
            let client = &self;
            async move {
                let resp: serde_json::Value = client
                    .build_request(reqwest::Method::GET, &format!("/records/{}", id))
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;

                Ok(ClientRecord {
                    id: resp["record"]["id"]
                        .as_str()
                        .or_else(|| resp["id"].as_str())
                        .unwrap_or("")
                        .to_string(),
                    content: resp["record"]["content"]
                        .as_str()
                        .or_else(|| resp["content"].as_str())
                        .unwrap_or("")
                        .to_string(),
                    content_type: resp["record"]["content_type"]
                        .as_str()
                        .or_else(|| resp["content_type"].as_str())
                        .unwrap_or("")
                        .to_string(),
                    tier: resp["tier"].as_str().unwrap_or("episodic").to_string(),
                    importance: resp["importance"].as_f64().unwrap_or(0.5),
                    access_count: resp["access_count"].as_u64().unwrap_or(0),
                    metadata: resp
                        .get("record")
                        .and_then(|r| r.get("metadata"))
                        .or_else(|| resp.get("metadata"))
                        .cloned()
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                    timestamp: resp["record"]["timestamp"]
                        .as_str()
                        .or_else(|| resp["timestamp"].as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            }
        })
        .await
    }

    pub async fn delete(&self, id: &str) -> Result<bool, String> {
        let id = id.to_string();
        self.execute_with_retry("delete", || {
            let id = id.clone();
            let client = &self;
            async move {
                let resp = client
                    .build_request(reqwest::Method::DELETE, &format!("/records/{}", id))
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?;
                Ok(resp.status().is_success())
            }
        })
        .await
    }

    // ── Search ───────────────────────────────────────────────────────────

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ClientSearchResult>, String> {
        let url = format!("/search?q={}&limit={}", urlencoding::encode(query), limit);
        self.execute_with_retry("search", || {
            let url = url.clone();
            let client = &self;
            async move {
                let resp: Vec<serde_json::Value> = client
                    .build_request(reqwest::Method::GET, &url)
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;

                Ok(resp
                    .into_iter()
                    .map(|r| ClientSearchResult {
                        id: r["record"]["id"]
                            .as_str()
                            .or_else(|| r["id"].as_str())
                            .unwrap_or("")
                            .to_string(),
                        content: r["record"]["content"]
                            .as_str()
                            .or_else(|| r["content"].as_str())
                            .unwrap_or("")
                            .to_string(),
                        content_type: r["record"]["content_type"]
                            .as_str()
                            .or_else(|| r["content_type"].as_str())
                            .unwrap_or("")
                            .to_string(),
                        tier: "episodic".to_string(),
                        score: r["score"].as_f64().unwrap_or(0.0),
                        method: r["method"].as_str().unwrap_or("fts").to_string(),
                        metadata: r
                            .get("record")
                            .and_then(|rec| rec.get("metadata"))
                            .cloned()
                            .and_then(|v| serde_json::from_value(v).ok())
                            .unwrap_or_default(),
                    })
                    .collect())
            }
        })
        .await
    }

    pub async fn search_tier(
        &self,
        query: &str,
        tier: &str,
        limit: usize,
    ) -> Result<Vec<ClientSearchResult>, String> {
        let url = format!(
            "/search?q={}&tier={}&limit={}",
            urlencoding::encode(query),
            tier,
            limit
        );
        let tier = tier.to_string();
        self.execute_with_retry("search_tier", || {
            let url = url.clone();
            let tier = tier.clone();
            let client = &self;
            async move {
                let resp: Vec<serde_json::Value> = client
                    .build_request(reqwest::Method::GET, &url)
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;

                Ok(resp
                    .into_iter()
                    .map(|r| ClientSearchResult {
                        id: r["record"]["id"]
                            .as_str()
                            .or_else(|| r["id"].as_str())
                            .unwrap_or("")
                            .to_string(),
                        content: r["record"]["content"]
                            .as_str()
                            .or_else(|| r["content"].as_str())
                            .unwrap_or("")
                            .to_string(),
                        content_type: r["record"]["content_type"]
                            .as_str()
                            .or_else(|| r["content_type"].as_str())
                            .unwrap_or("")
                            .to_string(),
                        tier: tier.clone(),
                        score: r["score"].as_f64().unwrap_or(0.0),
                        method: r["method"].as_str().unwrap_or("fts").to_string(),
                        metadata: r
                            .get("record")
                            .and_then(|rec| rec.get("metadata"))
                            .cloned()
                            .and_then(|v| serde_json::from_value(v).ok())
                            .unwrap_or_default(),
                    })
                    .collect())
            }
        })
        .await
    }

    // ── Tier Operations ──────────────────────────────────────────────────

    pub async fn promote(&self, id: &str, tier: &str) -> Result<(), String> {
        let path = format!("/tiers/promote/{}/{}", id, tier);
        self.execute_with_retry("promote", || {
            let path = path.clone();
            let client = &self;
            async move {
                client
                    .build_request(reqwest::Method::POST, &path)
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?;
                Ok(())
            }
        })
        .await
    }

    pub async fn flush_working(&self) -> Result<u64, String> {
        self.execute_with_retry("flush_working", || {
            let client = &self;
            async move {
                let resp: serde_json::Value = client
                    .build_request(reqwest::Method::POST, "/tiers/flush")
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;
                Ok(resp["flushed"].as_u64().unwrap_or(0))
            }
        })
        .await
    }

    // ── Graph ────────────────────────────────────────────────────────────

    pub async fn add_edge(
        &self,
        source_id: &str,
        target_id: &str,
        relation_type: &str,
        weight: f64,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "source_id": source_id,
            "target_id": target_id,
            "relation_type": relation_type,
            "weight": weight,
        });

        self.execute_with_retry("add_edge", || {
            let body = body.clone();
            let client = &self;
            async move {
                let resp: serde_json::Value = client
                    .build_request(reqwest::Method::POST, "/graph/edges")
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;
                Ok(resp["edge_id"].as_str().unwrap_or("").to_string())
            }
        })
        .await
    }

    // ── System ───────────────────────────────────────────────────────────

    pub async fn health(&self) -> Result<HealthStatus, String> {
        self.execute_with_retry("health", || {
            let client = &self;
            async move {
                let resp: serde_json::Value = client
                    .build_request(reqwest::Method::GET, "/health")
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;

                Ok(HealthStatus {
                    status: resp["status"].as_str().unwrap_or("unknown").to_string(),
                    total_records: resp["total_records"].as_u64().unwrap_or(0),
                    graph_edges: resp["graph_edges"].as_u64().unwrap_or(0),
                })
            }
        })
        .await
    }

    pub async fn stats(&self) -> Result<MemoryStats, String> {
        self.execute_with_retry("stats", || {
            let client = &self;
            async move {
                let resp: serde_json::Value = client
                    .build_request(reqwest::Method::GET, "/stats")
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("Parse error: {}", e))?;

                Ok(MemoryStats {
                    total_records: resp["total_records"].as_u64().unwrap_or(0),
                    total_with_embeddings: resp["total_with_embeddings"]
                        .as_u64()
                        .unwrap_or(0),
                    storage_bytes: resp["storage_bytes"].as_u64().unwrap_or(0),
                })
            }
        })
        .await
    }

    pub async fn clear(&self) -> Result<(), String> {
        self.execute_with_retry("clear", || {
            let client = &self;
            async move {
                client
                    .build_request(reqwest::Method::POST, "/clear")
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {}", e))?;
                Ok(())
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = MemoryClient::new("http://localhost:3111");
        assert_eq!(client.base_url, "http://localhost:3111");
        assert_eq!(client.max_retries, 3);
        assert_eq!(client.base_delay_ms, 500);
        assert_eq!(client.circuit_state(), "closed");
    }

    #[test]
    fn test_client_with_api_key() {
        let client = MemoryClient::new("http://localhost:3111").with_api_key("test-key");
        assert_eq!(client.api_key, Some("test-key".to_string()));
    }

    #[test]
    fn test_client_with_retry_config() {
        let client = MemoryClient::new("http://localhost:3111").with_retry(5, 1000);
        assert_eq!(client.max_retries, 5);
        assert_eq!(client.base_delay_ms, 1000);
    }

    #[test]
    fn test_client_with_circuit_breaker_config() {
        let client = MemoryClient::new("http://localhost:3111").with_circuit_breaker(10, 60);
        assert_eq!(client.circuit_state(), "closed");
    }

    #[test]
    fn test_url_construction() {
        let client = MemoryClient::new("http://localhost:3111");
        assert_eq!(client.url("/records"), "http://localhost:3111/records");
        assert_eq!(client.url("/health"), "http://localhost:3111/health");
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error("Request failed: connection refused"));
        assert!(is_retryable_error("Request failed: connection timed out"));
        assert!(is_retryable_error("Request failed: dns error"));
        assert!(is_retryable_error("Request failed: status=500 Internal Server Error"));
        assert!(is_retryable_error("Request failed: status=503 Service Unavailable"));
        assert!(!is_retryable_error("Request failed: status=400 Bad Request"));
        assert!(!is_retryable_error("Request failed: status=404 Not Found"));
        assert!(!is_retryable_error("Parse error: unexpected token"));
    }

    #[test]
    fn test_trailing_slash_stripped() {
        let client = MemoryClient::new("http://localhost:3111/");
        assert_eq!(client.base_url, "http://localhost:3111");
    }
}
