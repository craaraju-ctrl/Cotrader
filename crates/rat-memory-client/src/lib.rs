//! Agentic Memory Client
//!
//! Connects to Agentic Memory service (port 3111) for:
//! - Storing trading decisions, lessons, and context
//! - Recalling past decisions and patterns
//! - Long-term memory for self-evolution

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════════
// Errors
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: status={status} message={message}")]
    Api { status: u16, message: String },

    #[error("Serialization error: {0}")]
    Serialization(String),
}

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub metadata_filter: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entries: Vec<MemoryEntry>,
    pub total: u32,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Client
// ═══════════════════════════════════════════════════════════════════════════════

pub struct AgenticMemoryClient {
    base_url: String,
    http: reqwest::Client,
}

impl AgenticMemoryClient {
    pub fn new(base_url: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }

    /// Store a new memory entry
    pub async fn store(
        &self,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<MemoryEntry, MemoryError> {
        let body = serde_json::json!({
            "content": content,
            "metadata": metadata,
        });
        let resp: MemoryEntry = self.http
            .post(format!("{}/api/memories", self.base_url))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    /// Search memories by query
    pub async fn search(
        &self,
        query: &str,
        limit: Option<u32>,
    ) -> Result<SearchResult, MemoryError> {
        let body = SearchQuery {
            query: query.to_string(),
            limit,
            metadata_filter: None,
        };
        let resp: SearchResult = self.http
            .post(format!("{}/api/memories/search", self.base_url))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    /// Get a specific memory by ID
    pub async fn get(&self, id: &str) -> Result<MemoryEntry, MemoryError> {
        let resp: MemoryEntry = self.http
            .get(format!("{}/api/memories/{}", self.base_url, id))
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    /// Delete a memory by ID
    pub async fn delete(&self, id: &str) -> Result<(), MemoryError> {
        self.http
            .delete(format!("{}/api/memories/{}", self.base_url, id))
            .send()
            .await?;
        Ok(())
    }

    /// Store a trading decision with context
    pub async fn store_decision(
        &self,
        symbol: &str,
        action: &str,
        reasoning: &str,
        outcome: Option<&str>,
    ) -> Result<MemoryEntry, MemoryError> {
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "trading_decision".to_string());
        metadata.insert("symbol".to_string(), symbol.to_string());
        metadata.insert("action".to_string(), action.to_string());
        if let Some(outcome) = outcome {
            metadata.insert("outcome".to_string(), outcome.to_string());
        }

        let content = format!(
            "Decision: {} {} | Reasoning: {} | Outcome: {}",
            action,
            symbol,
            reasoning,
            outcome.unwrap_or("pending")
        );

        self.store(&content, metadata).await
    }

    /// Recall past decisions for a symbol
    pub async fn recall_decisions(
        &self,
        symbol: &str,
        limit: Option<u32>,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let query = format!("trading_decision {}", symbol);
        let result = self.search(&query, limit).await?;
        Ok(result.entries)
    }

    /// Store a lesson learned from a trade
    pub async fn store_lesson(
        &self,
        lesson: &str,
        symbol: &str,
        regret_score: f64,
    ) -> Result<MemoryEntry, MemoryError> {
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "lesson".to_string());
        metadata.insert("symbol".to_string(), symbol.to_string());
        metadata.insert("regret_score".to_string(), regret_score.to_string());

        let content = format!("Lesson (regret={:.2}): {}", regret_score, lesson);
        self.store(&content, metadata).await
    }

    /// Recall lessons for a symbol
    pub async fn recall_lessons(
        &self,
        symbol: &str,
        limit: Option<u32>,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let query = format!("lesson {}", symbol);
        let result = self.search(&query, limit).await?;
        Ok(result.entries)
    }

    /// Health check
    pub async fn health(&self) -> Result<bool, MemoryError> {
        let resp = self.http
            .get(format!("{}/api/health", self.base_url))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }
}
