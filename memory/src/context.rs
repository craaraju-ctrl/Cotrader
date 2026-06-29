//! # Context Window Manager — Letta/MemGPT-Style Paging
//!
//! Implements the MemGPT/Letta pattern of context window management:
//! - **Core Memory**: Pinned blocks always in context (persona, user prefs)
//! - **Recall Memory**: Recent conversation/interaction history
//! - **Archival Memory**: Vast long-term storage, retrieved on demand
//! - **Auto-Summary**: Evicted blocks are summarized for recall
//! - **Paging**: Agent can "page" data in/out of context window

use std::collections::HashMap;

use crate::store::MemoryStore;
use crate::types::{ContextBlock, ContextConfig, ContextSummary};

/// The context window manager handles memory paging.
pub struct ContextManager {
    store: MemoryStore,
    config: ContextConfig,
    /// In-memory active context blocks (the "context window")
    active_blocks: HashMap<String, ContextBlock>,
}

impl ContextManager {
    pub fn new(store: MemoryStore, config: ContextConfig) -> Self {
        let mut manager = Self {
            store,
            config,
            active_blocks: HashMap::new(),
        };
        manager.init_core_blocks();
        manager
    }

    pub fn with_defaults(store: MemoryStore) -> Self {
        Self::new(store, ContextConfig::default())
    }

    /// Initialize the default core memory blocks (persona + user).
    fn init_core_blocks(&mut self) {
        let now = chrono::Utc::now().to_rfc3339();

        self.active_blocks.insert(
            "persona".to_string(),
            ContextBlock {
                block_id: "persona".to_string(),
                label: "persona".to_string(),
                content: "I am a helpful AI assistant with long-term memory.".to_string(),
                pinned: true,
                priority: 100,
                max_tokens: 512,
                current_tokens: 12,
                last_updated: now.clone(),
                metadata: HashMap::new(),
            },
        );

        self.active_blocks.insert(
            "user".to_string(),
            ContextBlock {
                block_id: "user".to_string(),
                label: "user_preferences".to_string(),
                content: String::new(),
                pinned: true,
                priority: 99,
                max_tokens: 512,
                current_tokens: 0,
                last_updated: now,
                metadata: HashMap::new(),
            },
        );
    }

    /// Add or update a context block.
    /// Persists to SQLite and syncs active blocks.
    pub fn upsert_block(&mut self, block: ContextBlock) -> Result<(), String> {
        let current_tokens: usize = self.active_blocks.values().map(|b| b.current_tokens).sum();
        let block_tokens = block.content.split_whitespace().count();

        if current_tokens + block_tokens > self.config.max_tokens && !block.pinned {
            self.evict_to_fit(block_tokens)?;
        }

        self.active_blocks.insert(block.block_id.clone(), block.clone());

        if let Err(e) = self.store.store_context_block(&block) {
            tracing::error!("Failed to persist context block {}: {}", block.block_id, e);
        }

        Ok(())
    }

    /// Read the current context as a formatted string (what the LLM sees).
    pub fn render_context(&self) -> String {
        let mut parts = Vec::new();

        let mut pinned: Vec<&ContextBlock> = self
            .active_blocks
            .values()
            .filter(|b| b.pinned)
            .collect();
        pinned.sort_by_key(|b| std::cmp::Reverse(b.priority));

        for block in &pinned {
            if !block.content.is_empty() {
                parts.push(format!(
                    "[{}]\n{}",
                    block.label.to_uppercase(),
                    block.content
                ));
            }
        }

        let mut dynamic: Vec<&ContextBlock> = self
            .active_blocks
            .values()
            .filter(|b| !b.pinned)
            .collect();
        dynamic.sort_by_key(|b| std::cmp::Reverse(b.priority));

        for block in &dynamic {
            if !block.content.is_empty() {
                parts.push(format!(
                    "[{}]\n{}",
                    block.label.to_uppercase(),
                    block.content
                ));
            }
        }

        parts.join("\n\n")
    }

    /// Get total current token usage.
    pub fn token_usage(&self) -> usize {
        self.active_blocks.values().map(|b| b.current_tokens).sum()
    }

    /// Get available token budget.
    pub fn token_budget(&self) -> usize {
        self.config
            .max_tokens
            .saturating_sub(self.token_usage())
    }

    /// Evict lowest-priority non-pinned blocks to fit `needed_tokens`.
    fn evict_to_fit(&mut self, needed_tokens: usize) -> Result<(), String> {
        let mut freed = 0usize;
        let mut to_evict: Vec<String> = Vec::new();

        let mut dynamic: Vec<(&String, &ContextBlock)> = self
            .active_blocks
            .iter()
            .filter(|(_, b)| !b.pinned)
            .collect();
        dynamic.sort_by_key(|(_, b)| b.priority);

        for (id, block) in &dynamic {
            if freed >= needed_tokens {
                break;
            }
            freed += block.current_tokens;
            to_evict.push((*id).clone());
        }

        for id in &to_evict {
            self.active_blocks.remove(id);
            let _ = self.store.delete_context_block(id);
        }

        Ok(())
    }

    /// Archive a block's content as a summary (for later recall).
    /// Persists the summary to SQLite.
    pub fn archive_block(&self, block: &ContextBlock) -> ContextSummary {
        let summary = ContextSummary {
            summary_id: format!("sum_{}", crate::generate_id()),
            topic: block.label.clone(),
            summary: if block.content.len() > 500 {
                format!("{}...", &block.content[..500])
            } else {
                block.content.clone()
            },
            source_block_ids: vec![block.block_id.clone()],
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Err(e) = self.store.store_context_summary(&summary) {
            tracing::error!("Failed to persist context summary: {}", e);
        }

        summary
    }

    /// Get a specific block.
    pub fn get_block(&self, block_id: &str) -> Option<&ContextBlock> {
        self.active_blocks.get(block_id)
    }

    /// Get all active blocks.
    pub fn active_blocks(&self) -> &HashMap<String, ContextBlock> {
        &self.active_blocks
    }

    /// Sync all active blocks to SQLite (call after batch modifications).
    pub fn sync_to_store(&self) -> rusqlite::Result<()> {
        let blocks: Vec<ContextBlock> = self.active_blocks.values().cloned().collect();
        self.store.sync_context_blocks(&blocks)
    }

    /// Load context blocks from SQLite (call on startup).
    pub fn load_from_store(&mut self) -> rusqlite::Result<()> {
        let blocks = self.store.list_context_blocks()?;
        for block in blocks {
            self.active_blocks.insert(block.block_id.clone(), block);
        }
        Ok(())
    }

    /// Search archived context summaries from SQLite.
    pub fn search_summaries(&self, query: &str, limit: usize) -> Vec<ContextSummary> {
        self.store.search_context_summaries(query, limit).unwrap_or_default()
    }

    /// List recent archived summaries from SQLite.
    pub fn list_summaries(&self, limit: usize) -> Vec<ContextSummary> {
        self.store.list_context_summaries(limit).unwrap_or_default()
    }

    /// Get the context config.
    pub fn config(&self) -> &ContextConfig {
        &self.config
    }

    /// Remove a specific block by ID. Returns the removed block if it existed.
    pub fn remove_block(&mut self, block_id: &str) -> Option<ContextBlock> {
        self.active_blocks.remove(block_id)
    }

    /// Get the store reference.
    pub fn store(&self) -> &MemoryStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StorageConfig;

    fn setup() -> ContextManager {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        ContextManager::with_defaults(store)
    }

    #[test]
    fn test_init_core_blocks() {
        let manager = setup();
        assert!(manager.get_block("persona").is_some());
        assert!(manager.get_block("user").is_some());
    }

    #[test]
    fn test_render_context() {
        let manager = setup();
        let ctx = manager.render_context();
        assert!(ctx.contains("PERSONA"));
    }

    #[test]
    fn test_token_usage() {
        let manager = setup();
        let usage = manager.token_usage();
        assert!(usage > 0);
    }

    #[test]
    fn test_upsert_block() {
        let mut manager = setup();
        let block = ContextBlock {
            block_id: "recent".to_string(),
            label: "recent".to_string(),
            content: "Some recent context".to_string(),
            pinned: false,
            priority: 50,
            max_tokens: 1024,
            current_tokens: 3,
            last_updated: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };
        manager.upsert_block(block).unwrap();
        assert!(manager.get_block("recent").is_some());
    }

    #[test]
    fn test_archive_block() {
        let manager = setup();
        let block = manager.get_block("persona").unwrap();
        let summary = manager.archive_block(block);
        assert!(!summary.summary.is_empty());
    }
}
