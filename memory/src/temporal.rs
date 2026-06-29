//! # Temporal Memory — Versioned Facts with Ebbinghaus Decay
//!
//! Implements Zep-style temporal knowledge tracking:
//! - **Fact Versioning**: Each fact has a version history
//! - **Ebbinghaus Forgetting Curve**: Facts decay over time, boosted by recall
//! - **Temporal Invalidation**: Old versions are marked superseded
//! - **Recall Boosting**: Each recall resets the decay curve
//! - **Stale Detection**: Identify facts that have decayed below threshold

use crate::store::MemoryStore;
use crate::types::{DecayConfig, TemporalFact};

/// The temporal memory engine manages versioned facts with decay.
pub struct TemporalEngine {
    store: MemoryStore,
    decay_config: DecayConfig,
}

impl TemporalEngine {
    pub fn new(store: MemoryStore, decay_config: DecayConfig) -> Self {
        Self {
            store,
            decay_config,
        }
    }

    pub fn with_defaults(store: MemoryStore) -> Self {
        Self::new(store, DecayConfig::default())
    }

    /// Store a new fact, or create a new version if a similar fact exists.
    /// Persists to SQLite for long-term storage.
    pub fn store_fact(
        &self,
        content: &str,
        content_type: &str,
        importance: f64,
        metadata: std::collections::HashMap<String, String>,
    ) -> TemporalFact {
        let now = chrono::Utc::now().to_rfc3339();
        let fact_id = format!("fact_{}", crate::generate_id());

        let fact = TemporalFact {
            fact_id,
            content: content.to_string(),
            content_type: content_type.to_string(),
            valid_from: now.clone(),
            valid_to: None,
            sys_start: now.clone(),
            sys_end: None,
            version: 1,
            previous_version_id: None,
            decay_score: 1.0, // starts at full strength
            recall_count: 0,
            last_recalled: None,
            importance,
            metadata,
        };

        if let Err(e) = self.store.store_temporal_fact(&fact) {
            tracing::error!("Failed to persist temporal fact: {}", e);
        }

        fact
    }

    /// Update a fact, creating a new version and invalidating the old one.
    /// Persists both the old (invalidated) and new versions to SQLite.
    pub fn update_fact(
        &self,
        old_fact: &mut TemporalFact,
        new_content: &str,
        new_importance: Option<f64>,
    ) -> TemporalFact {
        let now = chrono::Utc::now().to_rfc3339();

        // Invalidate old version
        old_fact.valid_to = Some(now.clone());
        old_fact.sys_end = Some(now.clone());

        // Persist old version's invalidation
        let _ = self.store.invalidate_temporal_fact(&old_fact.fact_id);

        // Create new version
        let new_id = format!("fact_{}", crate::generate_id());
        let mut new_metadata = old_fact.metadata.clone();
        new_metadata.insert("previous_version".to_string(), old_fact.fact_id.clone());

        let new_fact = TemporalFact {
            fact_id: new_id,
            content: new_content.to_string(),
            content_type: old_fact.content_type.clone(),
            valid_from: now.clone(),
            valid_to: None,
            sys_start: now,
            sys_end: None,
            version: old_fact.version + 1,
            previous_version_id: Some(old_fact.fact_id.clone()),
            decay_score: 1.0,
            recall_count: 0,
            last_recalled: None,
            importance: new_importance.unwrap_or(old_fact.importance),
            metadata: new_metadata,
        };

        if let Err(e) = self.store.store_temporal_fact(&new_fact) {
            tracing::error!("Failed to persist new temporal fact version: {}", e);
        }

        new_fact
    }

    /// Recall a fact — boost its decay score (Ebbinghaus recall effect).
    /// Persists the updated decay state to SQLite.
    pub fn recall_fact(&self, fact: &mut TemporalFact) {
        let now = chrono::Utc::now().to_rfc3339();
        fact.recall_count += 1;
        fact.last_recalled = Some(now.clone());

        // Ebbinghaus recall boost: each recall resets decay toward 1.0
        let boost = self.decay_config.min_recall_boost;
        fact.decay_score = (fact.decay_score + boost).min(1.0);

        // More frequent recalls = stronger boost
        if fact.recall_count > 5 {
            fact.decay_score = (fact.decay_score + 0.1).min(1.0);
        }

        // Persist the updated decay state
        if let Err(e) = self.store.update_temporal_decay(
            &fact.fact_id,
            fact.decay_score,
            fact.recall_count,
            &now,
        ) {
            tracing::error!("Failed to persist temporal fact recall: {}", e);
        }
    }

    /// Calculate current decay score based on Ebbinghaus forgetting curve.
    /// R(t) = e^(-t / (S * k))
    /// where t = days since last recall, S = stability, k = decay constant
    pub fn calculate_decay(&self, fact: &TemporalFact) -> f64 {
        let last_recalled = fact
            .last_recalled
            .as_ref()
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let days_since_recall = match last_recalled {
            Some(ts) => {
                let now = chrono::Utc::now();
                (now - ts).num_days() as f64
            }
            None => {
                // Use creation date
                chrono::DateTime::parse_from_rfc3339(&fact.sys_start)
                    .ok()
                    .map(|dt| {
                        let now = chrono::Utc::now();
                        (now - dt.with_timezone(&chrono::Utc)).num_days() as f64
                    })
                    .unwrap_or(30.0)
            }
        };

        // Stability increases with importance and recall count
        let stability = (1.0 + fact.importance * 2.0 + (fact.recall_count as f64 * 0.1)).max(1.0);

        // Ebbinghaus curve: R(t) = e^(-t / (S * half_life))
        let decay_rate = days_since_recall / (stability * self.decay_config.half_life_days);
        let decay = (-decay_rate * self.decay_config.acceleration).exp();

        // Clamp to [0, 1]
        decay.clamp(0.0, 1.0)
    }

    /// Check if a fact is stale (below decay threshold).
    pub fn is_stale(&self, fact: &TemporalFact) -> bool {
        let current_decay = self.calculate_decay(fact);
        current_decay < self.decay_config.stale_threshold
    }

    /// Get the effective importance of a fact (importance × decay).
    pub fn effective_importance(&self, fact: &TemporalFact) -> f64 {
        let decay = self.calculate_decay(fact);
        (fact.importance * decay).clamp(0.0, 1.0)
    }

    /// Get the version history chain for a fact.
    pub fn version_history(&self, fact: &TemporalFact) -> Vec<String> {
        let mut history = Vec::new();
        let mut current = Some(fact.version);
        while let Some(v) = current {
            history.push(format!("v{}", v));
            current = if v > 1 { Some(v - 1) } else { None };
        }
        history.reverse();
        history
    }

    /// Get the decay config.
    pub fn decay_config(&self) -> &DecayConfig {
        &self.decay_config
    }

    /// Retrieve a temporal fact by ID from SQLite.
    pub fn get_fact(&self, id: &str) -> Option<TemporalFact> {
        self.store.get_temporal_fact(id).ok().flatten()
    }

    /// List current (non-superseded) temporal facts from SQLite.
    pub fn list_current_facts(&self, content_type: Option<&str>, limit: usize) -> Vec<TemporalFact> {
        self.store.list_current_temporal_facts(content_type, limit).unwrap_or_default()
    }

    /// Search temporal facts by content from SQLite.
    pub fn search_facts(&self, query: &str, limit: usize) -> Vec<TemporalFact> {
        self.store.search_temporal_facts(query, limit).unwrap_or_default()
    }

    /// Get version chain for a fact from SQLite.
    pub fn get_version_chain(&self, fact_id: &str) -> Vec<TemporalFact> {
        self.store.get_temporal_version_chain(fact_id).unwrap_or_default()
    }

    /// Delete a temporal fact from SQLite.
    pub fn delete_fact(&self, id: &str) -> bool {
        self.store.delete_temporal_fact(id).unwrap_or(false)
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

    fn setup() -> TemporalEngine {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        TemporalEngine::with_defaults(store)
    }

    #[test]
    fn test_store_fact() {
        let engine = setup();
        let fact = engine.store_fact(
            "The sky is blue",
            "fact",
            0.8,
            std::collections::HashMap::new(),
        );
        assert_eq!(fact.content, "The sky is blue");
        assert_eq!(fact.version, 1);
        assert!(fact.decay_score > 0.9);
    }

    #[test]
    fn test_recall_boost() {
        let engine = setup();
        let mut fact = engine.store_fact(
            "Important fact",
            "fact",
            0.9,
            std::collections::HashMap::new(),
        );
        fact.decay_score = 0.5; // simulate decayed fact

        engine.recall_fact(&mut fact);
        assert!(fact.decay_score > 0.5);
        assert_eq!(fact.recall_count, 1);
    }

    #[test]
    fn test_effective_importance() {
        let engine = setup();
        let mut fact = engine.store_fact(
            "Test fact",
            "fact",
            1.0,
            std::collections::HashMap::new(),
        );
        // A new fact should have full effective importance
        let eff = engine.effective_importance(&fact);
        assert!(eff > 0.9);

        // After recall, importance should remain high
        engine.recall_fact(&mut fact);
        let eff2 = engine.effective_importance(&fact);
        assert!(eff2 > 0.9);
    }

    #[test]
    fn test_version_history() {
        let engine = setup();
        let fact = engine.store_fact(
            "Versioned fact",
            "fact",
            0.7,
            std::collections::HashMap::new(),
        );
        let history = engine.version_history(&fact);
        assert_eq!(history, vec!["v1"]);
    }
}
