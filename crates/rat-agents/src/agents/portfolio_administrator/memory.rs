//! portfolio_administrator Memory — Agent-specific memory connection.

use agentic_memory::client::MemoryClient;

pub struct portfolio_administratorMemory {
    client: MemoryClient,
    namespace: String,
}

impl portfolio_administratorMemory {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: MemoryClient::new(base_url),
            namespace: "portfolio_administrator".to_string(),
        }
    }

    pub fn from_env() -> Self {
        Self {
            client: MemoryClient::from_env(),
            namespace: "portfolio_administrator".to_string(),
        }
    }

    pub async fn remember(&self, content: &str, content_type: &str, tier: &str, importance: f64) -> Result<String, String> {
        let id = format!("{}/{}", self.namespace, uuid::Uuid::new_v4());
        self.client.insert_with_id(&id, content, content_type, tier, importance).await
    }

    pub async fn recall(&self, query: &str, limit: usize) -> Result<Vec<agentic_memory::client::ClientSearchResult>, String> {
        self.client.search(query, limit).await
    }

    pub async fn get_memory(&self, id: &str) -> Result<agentic_memory::client::ClientRecord, String> {
        self.client.get(id).await
    }

    pub async fn forget(&self, id: &str) -> Result<bool, String> {
        self.client.delete(id).await
    }

    pub async fn stats(&self) -> Result<agentic_memory::client::MemoryStats, String> {
        self.client.stats().await
    }
}
