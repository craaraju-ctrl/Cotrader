//! # Memory Consolidation Engine
//!
//! Transforms raw, noisy data into stable, long-term memory through:
//!
//! - **Importance Scoring** — Rank records by access frequency, recency, connections
//! - **Fact Extraction** — Distill key facts from records for semantic memory
//! - **Deduplication** — Find and merge similar records
//! - **Conflict Detection** — Identify contradictory facts
//! - **Summarization** — Compress episodic records into semantic knowledge
//! - **Promotion Pipeline** — Move important records up the tier hierarchy

use serde::{Deserialize, Serialize};
use crate::store::MemoryStore;
use crate::temporal::TemporalEngine;
use crate::types::{
    ConsolidationReport, ImportanceContext, MemoryRecord, MemoryTier, TieredRecord,
};

/// Default importance scorer based on access patterns, recency, and metadata.
pub struct DefaultImportanceScorer;

impl DefaultImportanceScorer {
    pub fn new() -> Self {
        Self
    }

    /// Compute an importance score (0.0–1.0) for a record.
    pub fn score(&self, context: &ImportanceContext) -> f64 {
        let mut score = 0.0;

        // Access frequency (up to 0.3)
        score += (context.access_count as f64).min(100.0) / 100.0 * 0.3;

        // Recency (up to 0.25) — newer = higher
        let recency_factor = (-context.age_seconds / 86400.0_f64).exp(); // decay over days
        score += recency_factor * 0.25;

        // Has embedding (0.1)
        if context.has_embedding {
            score += 0.1;
        }

        // Content length bonus (up to 0.1)
        let length_factor = (context.content_length as f64).min(1000.0) / 1000.0;
        score += length_factor * 0.1;

        // Content type weight (up to 0.1)
        match context.content_type.as_str() {
            "procedure" | "insight" | "lesson" => score += 0.1,
            "fact" | "semantic" => score += 0.08,
            "knowledge" => score += 0.05,
            _ => {}
        }

        // Graph connections (up to 0.1)
        score += (context.graph_connections as f64).min(20.0) / 20.0 * 0.1;

        // Expert endorsements (up to 0.05)
        score += (context.expert_endorsements as f64).min(5.0) / 5.0 * 0.05;

        score.clamp(0.0, 1.0)
    }

    /// Compute importance for a tiered record from the store.
    pub fn score_record(&self, record: &TieredRecord, store: &MemoryStore) -> f64 {
        let age_seconds = match chrono::DateTime::parse_from_rfc3339(&record.record.timestamp) {
            Ok(dt) => {
                let now = chrono::Utc::now();
                let dt_utc = dt.with_timezone(&chrono::Utc);
                (now - dt_utc).num_seconds().max(0) as f64
            }
            Err(_) => 86400.0, // assume 1 day old if parse fails
        };

        // Count graph connections via edges query on the SQLite store
        let graph_connections = store
            .get_edges(&record.record.id)
            .map(|e| e.len())
            .unwrap_or(0);

        let context = ImportanceContext {
            access_count: record.access_count,
            age_seconds,
            has_embedding: record.record.embedding.is_some(),
            content_length: record.record.content.len(),
            content_type: record.record.content_type.clone(),
            tier: record.tier,
            graph_connections,
            expert_endorsements: 0,
        };

        self.score(&context)
    }
}

impl Default for DefaultImportanceScorer {
    fn default() -> Self {
        Self::new()
    }
}

/// Financial-aware importance scorer for trading memory.
/// Binds memory longevity to actual portfolio impact.
pub struct FinancialRegretScorer {
    /// Weight for regret score (0.0-1.0)
    pub regret_weight: f64,
    /// Weight for balance delta impact
    pub balance_delta_weight: f64,
    /// Weight for position size impact
    pub position_weight: f64,
    /// Weight for regime change significance
    pub regime_weight: f64,
}

impl Default for FinancialRegretScorer {
    fn default() -> Self {
        Self {
            regret_weight: 0.35,
            balance_delta_weight: 0.25,
            position_weight: 0.20,
            regime_weight: 0.20,
        }
    }
}

impl FinancialRegretScorer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute importance score for a financial memory record.
    /// Formula: Access_Factor + Recency_Factor + (w1 * regret) + (w2 * log10(delta))
    pub fn score(&self, context: &ImportanceContext, metadata: &std::collections::HashMap<String, String>) -> f64 {
        let mut score = 0.0;

        // Access factor (up to 0.3)
        score += (context.access_count as f64).min(100.0) / 100.0 * 0.3;

        // Recency factor (up to 0.25)
        let recency_factor = (-context.age_seconds / 86400.0_f64).exp();
        score += recency_factor * 0.25;

        // Financial components from metadata
        let regret_score = metadata.get("regret_score")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.5);

        let balance_delta = metadata.get("balance_delta")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        let position_size = metadata.get("position_size")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        let regime = metadata.get("regime")
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        let is_win = metadata.get("is_win")
            .map(|s| s == "true")
            .unwrap_or(true);

        let leverage = metadata.get("leverage")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        // Regret component: high regret = high importance
        score += regret_score * self.regret_weight;

        // Balance delta: log-scale impact
        let delta_impact = if balance_delta.abs() > 0.0 {
            (balance_delta.abs() + 1.0).log10().clamp(0.0, 5.0) / 5.0
        } else {
            0.0
        };
        score += delta_impact * self.balance_delta_weight;

        // Position size: larger positions = more memorable
        let position_impact = (position_size / 100000.0).clamp(0.0, 1.0);
        score += position_impact * self.position_weight;

        // Regime significance
        let regime_impact = match regime.as_str() {
            "Volatile" | "HighVolatility" => 1.0,
            "TrendingBull" | "TrendingBear" => 0.7,
            "Ranging" => 0.4,
            _ => 0.3,
        };
        score += regime_impact * self.regime_weight;

        // Leverage amplifier
        if leverage > 10 {
            score *= 1.0 + (leverage as f64 / 100.0);
        }

        // Loss asymmetry: losses remembered more strongly
        if !is_win {
            score *= 1.2;
        }

        score.clamp(0.0, 1.0)
    }

    /// Score from a tiered record using its metadata.
    pub fn score_record(&self, record: &TieredRecord, store: &MemoryStore) -> f64 {
        let age_seconds = match chrono::DateTime::parse_from_rfc3339(&record.record.timestamp) {
            Ok(dt) => {
                let now = chrono::Utc::now();
                let dt_utc = dt.with_timezone(&chrono::Utc);
                (now - dt_utc).num_seconds().max(0) as f64
            }
            Err(_) => 86400.0,
        };

        let graph_connections = store
            .get_edges(&record.record.id)
            .map(|e| e.len())
            .unwrap_or(0);

        let context = ImportanceContext {
            access_count: record.access_count,
            age_seconds,
            has_embedding: record.record.embedding.is_some(),
            content_length: record.record.content.len(),
            content_type: record.record.content_type.clone(),
            tier: record.tier,
            graph_connections,
            expert_endorsements: 0,
        };

        self.score(&context, &record.record.metadata)
    }
}


/// The consolidation engine runs periodic cycles to optimize memory.
pub struct ConsolidationEngine {
    store: MemoryStore,
    scorer: DefaultImportanceScorer,
    temporal: TemporalEngine,
}

impl ConsolidationEngine {
    pub fn new(store: MemoryStore) -> Self {
        Self {
            store: store.clone(),
            scorer: DefaultImportanceScorer::new(),
            temporal: TemporalEngine::with_defaults(store),
        }
    }

    /// Create a consolidation engine with custom temporal decay configuration.
    pub fn with_decay_config(store: MemoryStore, decay_config: crate::types::DecayConfig) -> Self {
        Self {
            store: store.clone(),
            scorer: DefaultImportanceScorer::new(),
            temporal: TemporalEngine::new(store, decay_config),
        }
    }

    /// Run a full consolidation cycle.
    /// This processes records across all tiers and produces a report.
    pub fn run_cycle(&self) -> ConsolidationReport {
        let started_at = chrono::Utc::now().to_rfc3339();
        let start_time = std::time::Instant::now();

        let mut report = ConsolidationReport {
            cycle_id: format!("cycle_{}", crate::generate_id()),
            started_at: started_at.clone(),
            completed_at: String::new(),
            records_processed: 0,
            records_extracted: 0,
            records_deduplicated: 0,
            records_merged: 0,
            records_summarized: 0,
            records_promoted: 0,
            records_demoted: 0,
            records_evicted: 0,
            conflicts_detected: 0,
            conflicts_resolved: 0,
            insights_generated: Vec::new(),
            duration_ms: 0,
        };

        // Phase 1: Update importance scores for all records (temporal-decay aware)
        if let Ok(records) = self.store.all(10000, 0) {
            report.records_processed = records.len() as u64;
            for record in &records {
                if let Ok(Some(tiered)) = self.store.get_tiered(&record.id) {
                    let base_importance = self.scorer.score_record(&tiered, &self.store);

                    // Apply temporal decay: records that haven't been recalled decay
                    let fact = tiered.to_temporal_fact();

                    // Blend: 60% base scoring + 40% decay-adjusted effective importance
                    let decay = self.temporal.calculate_decay(&fact);
                    let effective = (base_importance * decay).clamp(0.0, 1.0);
                    let new_importance = (base_importance * 0.6 + effective * 0.4).clamp(0.0, 1.0);

                    let _ = self.store.update_importance(&record.id, new_importance);
                }
            }
        }

        // Phase 2: Promote high-importance records
        for tier in &[
            MemoryTier::Working,
            MemoryTier::Episodic,
            MemoryTier::Semantic,
        ] {
            if let Ok(config) = self.store.get_tier_config(*tier) {
                if config.auto_promote {
                    if let Ok(records) = self.store.list_by_tier(*tier, 1000, 0) {
                        for tiered in &records {
                            if tiered.importance >= config.promotion_threshold {
                                if let Some(next_tier) = tiered.tier.promote_to() {
                                    if self
                                        .store
                                        .promote(&tiered.record.id, next_tier)
                                        .unwrap_or(false)
                                    {
                                        report.records_promoted += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Phase 3: Demote low-importance records
        for tier in &[
            MemoryTier::Procedural,
            MemoryTier::Semantic,
            MemoryTier::Episodic,
        ] {
            if let Ok(config) = self.store.get_tier_config(*tier) {
                if let Ok(records) = self.store.list_by_tier(*tier, 1000, 0) {
                    for tiered in &records {
                        if tiered.importance < config.demotion_threshold {
                            if let Some(prev_tier) = tiered.tier.demote_to() {
                                if self
                                    .store
                                    .promote(&tiered.record.id, prev_tier)
                                    .unwrap_or(false)
                                {
                                    report.records_demoted += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Phase 4: Deduplicate similar records within each tier
        for tier in MemoryTier::all() {
            if let Ok(records) = self.store.list_by_tier(tier, 1000, 0) {
                let dedup_count = self.deduplicate_tier(&records);
                report.records_deduplicated += dedup_count as u64;
            }
        }

        // Phase 5: Evict excess records from each tier
        for tier in MemoryTier::all() {
            if let Ok(config) = self.store.get_tier_config(tier) {
                if let Ok(evicted) = self.store.evict_from_tier(tier, config.max_records) {
                    report.records_evicted += evicted;
                }
            }
        }

        report.completed_at = chrono::Utc::now().to_rfc3339();
        report.duration_ms = start_time.elapsed().as_millis() as u64;
        report
    }

    /// Detect and merge duplicate records within a list.
    /// Two records are considered duplicates if they have the same content_type
    /// and high content similarity (simple heuristic: shared words ratio).
    fn deduplicate_tier(&self, records: &[TieredRecord]) -> usize {
        let mut dedup_count = 0;
        let mut to_delete: Vec<String> = Vec::new();

        for i in 0..records.len() {
            if to_delete.contains(&records[i].record.id) {
                continue;
            }
            for j in (i + 1)..records.len() {
                if to_delete.contains(&records[j].record.id) {
                    continue;
                }

                let a = &records[i];
                let b = &records[j];

                // Same content type and similar content?
                if a.record.content_type != b.record.content_type {
                    continue;
                }

                if self.content_similarity(&a.record.content, &b.record.content) > 0.8 {
                    // Decay-aware dedup: keep the record with higher effective importance
                    // (importance × temporal decay). A recently recalled record should win
                    // over an identical but forgotten one.
                    let a_decay = self.temporal.calculate_decay(&a.to_temporal_fact());
                    let b_decay = self.temporal.calculate_decay(&b.to_temporal_fact());
                    let a_effective = a.importance * a_decay;
                    let b_effective = b.importance * b_decay;

                    if a_effective >= b_effective {
                        to_delete.push(b.record.id.clone());
                    } else {
                        to_delete.push(a.record.id.clone());
                    }
                    dedup_count += 1;
                }
            }
        }

        for id in &to_delete {
            let _ = self.store.delete(id);
        }

        dedup_count
    }

    /// Simple content similarity: ratio of shared words (Jaccard-like).
    fn content_similarity(&self, a: &str, b: &str) -> f64 {
        let words_a: Vec<String> = a
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let words_b: Vec<String> = b
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if words_a.is_empty() && words_b.is_empty() {
            return 1.0;
        }
        if words_a.is_empty() || words_b.is_empty() {
            return 0.0;
        }

        let set_a: std::collections::HashSet<String> = words_a.into_iter().collect();
        let set_b: std::collections::HashSet<String> = words_b.into_iter().collect();

        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();

        intersection as f64 / union as f64
    }

    /// Merge two memory records (keeping higher importance fields).
    pub fn merge_records(
        &self,
        keep_id: &str,
        merge_id: &str,
    ) -> rusqlite::Result<Option<MemoryRecord>> {
        let keep = self.store.get(keep_id)?;
        let merge = self.store.get(merge_id)?;

        match (keep, merge) {
            (Some(mut k), Some(m)) => {
                // Combine content
                if !m.content.contains(&k.content) {
                    k.content = format!("{}\n\n{}", k.content, m.content);
                }

                // Merge metadata
                for (key, value) in &m.metadata {
                    k.metadata
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }

                // Take the earlier timestamp
                if m.timestamp < k.timestamp {
                    k.timestamp = m.timestamp;
                }

                // Save merged record
                self.store.insert(&k)?;
                // Delete the merged-from record
                self.store.delete(merge_id)?;

                Ok(Some(k))
            }
            (Some(k), None) => Ok(Some(k)),
            _ => Ok(None),
        }
    }

    /// Detect potential conflicts — records of same type with contradictory content.
    pub fn detect_conflicts(
        &self,
        content_type: &str,
    ) -> rusqlite::Result<Vec<(MemoryRecord, MemoryRecord)>> {
        let records = self.store.list_by_type(content_type, 1000, 0)?;
        let mut conflicts = Vec::new();

        for i in 0..records.len() {
            for j in (i + 1)..records.len() {
                let a = &records[i];
                let b = &records[j];

                // Check for negation-based conflicts
                let a_lower = a.content.to_lowercase();
                let b_lower = b.content.to_lowercase();

                let has_negation_a = a_lower.contains("not ")
                    || a_lower.contains("never ")
                    || a_lower.contains("no ");
                let has_negation_b = b_lower.contains("not ")
                    || b_lower.contains("never ")
                    || b_lower.contains("no ");

                if has_negation_a != has_negation_b {
                    // Check if they share key terms (potential conflict)
                    let shared = self.content_similarity(&a.content, &b.content);
                    if shared > 0.4 && shared < 0.9 {
                        conflicts.push((a.clone(), b.clone()));
                    }
                }
            }
        }

        Ok(conflicts)
    }

    /// Get a reference to the underlying store.
    pub fn store(&self) -> &MemoryStore {
        &self.store
    }

    /// Get a reference to the temporal engine.
    pub fn temporal(&self) -> &TemporalEngine {
        &self.temporal
    }
}

/// Game-theoretic conflict resolver for cross-namespace contradictions.
/// Maintains per-namespace accuracy and variance metrics to resolve disputes.
pub struct NamespaceArbitrator {
    /// Rolling accuracy per namespace (EMA)
    namespace_accuracy: std::collections::HashMap<String, f64>,
    /// Rolling error variance per namespace
    namespace_variance: std::collections::HashMap<String, f64>,
}

/// Result of a conflict arbitration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrationResult {
    pub winner_namespace: String,
    pub loser_namespace: String,
    pub confidence: f64,
    pub winner_score: f64,
    pub loser_score: f64,
    pub reasoning: String,
}

impl NamespaceArbitrator {
    pub fn new() -> Self {
        Self {
            namespace_accuracy: std::collections::HashMap::new(),
            namespace_variance: std::collections::HashMap::new(),
        }
    }

    /// Record an outcome for a namespace (was the prediction correct?).
    pub fn record_outcome(&mut self, namespace: &str, was_correct: bool) {
        let acc = self.namespace_accuracy
            .entry(namespace.to_string())
            .or_insert(0.5);
        // EMA: 90% old, 10% new
        *acc = *acc * 0.9 + (if was_correct { 1.0 } else { 0.0 }) * 0.1;
    }

    /// Record a prediction error for variance tracking.
    pub fn record_prediction(&mut self, namespace: &str, predicted: f64, actual: f64) {
        let error = predicted - actual;
        let var = self.namespace_variance
            .entry(namespace.to_string())
            .or_insert(0.1);
        // EMA of squared error
        *var = *var * 0.9 + error * error * 0.1;
    }

    /// Resolve a conflict between two namespaces using game-theoretic scoring.
    /// Returns the winning namespace and confidence.
    pub fn resolve_conflict(&self, ns_a: &str, ns_b: &str) -> ArbitrationResult {
        let acc_a = self.namespace_accuracy.get(ns_a).copied().unwrap_or(0.5);
        let acc_b = self.namespace_accuracy.get(ns_b).copied().unwrap_or(0.5);
        let var_a = self.namespace_variance.get(ns_a).copied().unwrap_or(0.1).max(0.05);
        let var_b = self.namespace_variance.get(ns_b).copied().unwrap_or(0.1).max(0.05);

        // Sharpe-like scoring: accuracy / variance (lower variance = more reliable)
        let score_a = acc_a / (1.0 + var_a);
        let score_b = acc_b / (1.0 + var_b);

        let (winner, loser, winner_score, loser_score) = if score_a >= score_b {
            (ns_a.to_string(), ns_b.to_string(), score_a, score_b)
        } else {
            (ns_b.to_string(), ns_a.to_string(), score_b, score_a)
        };

        let total = score_a + score_b;
        let confidence = if total > 0.0 { winner_score / total } else { 0.5 };

        ArbitrationResult {
            winner_namespace: winner,
            loser_namespace: loser,
            confidence,
            winner_score,
            loser_score,
            reasoning: format!(
                "Resolved by variance-weighted accuracy: {} (score={:.4}) > {} (score={:.4})",
                ns_a, score_a, ns_b, score_b
            ),
        }
    }

    /// Get current stats for a namespace.
    pub fn get_stats(&self, namespace: &str) -> (f64, f64) {
        let acc = self.namespace_accuracy.get(namespace).copied().unwrap_or(0.5);
        let var = self.namespace_variance.get(namespace).copied().unwrap_or(0.1);
        (acc, var)
    }
}

impl Default for NamespaceArbitrator {
    fn default() -> Self {
        Self::new()
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StorageConfig;

    fn setup() -> (MemoryStore, ConsolidationEngine) {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        let engine = ConsolidationEngine::new(store.clone());
        (store, engine)
    }

    #[test]
    fn test_importance_scoring() {
        let scorer = DefaultImportanceScorer::new();

        let ctx = ImportanceContext {
            access_count: 50,
            age_seconds: 3600.0, // 1 hour old
            has_embedding: true,
            content_length: 500,
            content_type: "fact".into(),
            tier: MemoryTier::Semantic,
            graph_connections: 5,
            expert_endorsements: 2,
        };

        let score = scorer.score(&ctx);
        assert!(score > 0.0 && score <= 1.0);
        assert!(score > 0.5); // Should be fairly important
    }

    #[test]
    fn test_content_similarity() {
        let (_, engine) = setup();
        let sim = engine.content_similarity(
            "Bitcoin reaches all time high",
            "Bitcoin hits new all time high",
        );
        assert!(sim > 0.5);

        let diff =
            engine.content_similarity("Bitcoin reaches all time high", "Ethereum merge completed");
        assert!(diff < 0.3);
    }

    #[test]
    fn test_detect_conflicts() {
        let (store, engine) = setup();

        store
            .insert(&MemoryRecord::new(
                "c1".into(),
                "The market is bullish".into(),
                "opinion".into(),
            ))
            .unwrap();
        store
            .insert(&MemoryRecord::new(
                "c2".into(),
                "The market is not bullish".into(),
                "opinion".into(),
            ))
            .unwrap();

        let conflicts = engine.detect_conflicts("opinion").unwrap();
        assert!(!conflicts.is_empty());
    }

    #[test]
    fn test_consolidation_cycle_runs() {
        let (store, engine) = setup();

        // Insert some records
        for i in 0..5 {
            let r = MemoryRecord::new(
                format!("cyc{}", i),
                format!("Record {} content here for testing", i),
                "cycle_test".into(),
            );
            store
                .insert_into_tier(&r, MemoryTier::Episodic, 0.5, None, None)
                .unwrap();
        }

        let report = engine.run_cycle();
        assert!(report.records_processed >= 5);
        assert!(report.completed_at > report.started_at);
    }
}
