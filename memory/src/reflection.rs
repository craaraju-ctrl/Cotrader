//! # Reflection Engine — Self-Reflective Memory with Inner Monologue
//!
//! Implements the MemGPT/Letta pattern of agent self-reflection:
//! - **Inner Monologue**: The agent reasons about its own memory state
//! - **Self-Assessment**: Periodic health checks on memory quality
//! - **Reflexion Loop**: Reflect → plan → act → observe → reflect
//! - **Adaptive Learning**: Adjust behavior based on reflection outcomes
//!
//! This module enables the memory system to "think about thinking" —
//! evaluating what memories are valuable, which are stale, and what
//! patterns emerge from the agent's experience.

use crate::store::MemoryStore;
use crate::types::{Reflection, SelfAssessment};

/// The reflection engine manages self-reflective memory operations.
pub struct ReflectionEngine {
    store: MemoryStore,
}

impl ReflectionEngine {
    pub fn new(store: MemoryStore) -> Self {
        Self { store }
    }

    /// Record a reflection — the agent's inner monologue about its state.
    /// Persists to SQLite for long-term storage.
    pub fn reflect(
        &self,
        topic: &str,
        monologue: &str,
        conclusion: &str,
        planned_actions: Vec<String>,
        confidence: f64,
        tags: Vec<String>,
    ) -> Reflection {
        let reflection = Reflection {
            reflection_id: format!("refl_{}", crate::generate_id()),
            topic: topic.to_string(),
            monologue: monologue.to_string(),
            conclusion: conclusion.to_string(),
            planned_actions,
            outcome: None,
            confidence,
            tags,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Err(e) = self.store.store_reflection(&reflection) {
            tracing::error!("Failed to persist reflection: {}", e);
        }

        reflection
    }

    /// Run a full self-assessment of the memory system's health.
    /// Persists the assessment to SQLite.
    pub fn self_assess(&self) -> SelfAssessment {
        let stats = self.store.stats().unwrap_or_else(|_| {
            crate::types::MemoryStats {
                total_records: 0,
                total_with_embeddings: 0,
                content_types: std::collections::HashMap::new(),
                storage_bytes: 0,
                tier_breakdown: std::collections::HashMap::new(),
            }
        });

        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        // Evaluate memory quality
        let mut memory_quality = 1.0f64;
        if stats.total_records == 0 {
            memory_quality = 0.0;
            issues.push("Memory is empty — no records stored".to_string());
            recommendations.push("Insert some memories to begin tracking".to_string());
        }

        // Evaluate tier distribution
        let mut coherence = 1.0f64;
        let tier_names: Vec<String> = stats.tier_breakdown.keys().cloned().collect();
        let active_tiers = tier_names.len();
        if active_tiers < 2 {
            coherence *= 0.7;
            issues.push(format!(
                "Only {} tier(s) have records — memory is not well-distributed",
                active_tiers
            ));
            recommendations
                .push("Spread memories across multiple tiers for better organization".to_string());
        }

        // Evaluate staleness (avg importance as proxy)
        let mut avg_importance = 0.0f64;
        let mut tier_count = 0u64;
        for tier_stats in stats.tier_breakdown.values() {
            if tier_stats.total_records > 0 {
                avg_importance += tier_stats.average_importance;
                tier_count += 1;
            }
        }
        if tier_count > 0 {
            avg_importance /= tier_count as f64;
        }

        let staleness = 1.0 - avg_importance;
        if staleness > 0.5 {
            issues.push(format!(
                "Average importance is low ({:.2}) — many records may be stale",
                avg_importance
            ));
            recommendations.push("Run consolidation to promote important records".to_string());
        }

        // Evaluate diversity (content type spread)
        let diversity = if stats.content_types.len() >= 3 {
            1.0
        } else {
            let d = stats.content_types.len() as f64 / 3.0;
            issues.push(format!(
                "Only {} content type(s) present — low diversity",
                stats.content_types.len()
            ));
            recommendations
                .push("Store different types of content (facts, events, procedures)".to_string());
            d
        };

        let overall = (memory_quality * 0.3 + coherence * 0.25 + (1.0 - staleness) * 0.25 + diversity * 0.2).clamp(0.0, 1.0);

        let assessment = SelfAssessment {
            assessment_id: format!("assess_{}", crate::generate_id()),
            memory_quality_score: memory_quality,
            coherence_score: coherence,
            staleness_score: staleness,
            diversity_score: diversity,
            overall_health: overall,
            issues_detected: issues,
            recommendations,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Err(e) = self.store.store_assessment(&assessment) {
            tracing::error!("Failed to persist self-assessment: {}", e);
        }

        assessment
    }

    /// Run the reflexion loop: reflect → plan → (agent acts) → observe outcome.
    /// This is the core self-improvement mechanism.
    pub fn reflexion_loop(
        &self,
        topic: &str,
        observation: &str,
    ) -> (Reflection, SelfAssessment) {
        // Step 1: Self-assess current state
        let assessment = self.self_assess();

        // Step 2: Generate reflection based on assessment
        let mut monologue = format!("Observing: {}", observation);
        monologue.push_str(&format!(
            "\n\nHealth assessment: {:.0}% overall",
            assessment.overall_health * 100.0
        ));

        if !assessment.issues_detected.is_empty() {
            monologue.push_str("\n\nIssues detected:");
            for issue in &assessment.issues_detected {
                monologue.push_str(&format!("\n  - {}", issue));
            }
        }

        let mut planned_actions = Vec::new();
        for rec in &assessment.recommendations {
            planned_actions.push(rec.clone());
            monologue.push_str(&format!("\n\nPlanned action: {}", rec));
        }

        let conclusion = if assessment.overall_health > 0.7 {
            "Memory system is healthy. Continue current patterns.".to_string()
        } else if assessment.overall_health > 0.4 {
            "Memory system needs moderate attention. Apply recommended improvements.".to_string()
        } else {
            "Memory system is in poor health. Urgent remediation needed.".to_string()
        };

        let reflection = self.reflect(
            topic,
            &monologue,
            &conclusion,
            planned_actions,
            assessment.overall_health,
            vec!["reflexion".to_string(), "self-assessment".to_string()],
        );

        (reflection, assessment)
    }

    /// Analyze reflection patterns over time to detect trends.
    pub fn analyze_patterns(&self, reflections: &[Reflection]) -> Vec<String> {
        let mut patterns = Vec::new();

        if reflections.is_empty() {
            return patterns;
        }

        // Detect recurring topics
        let mut topic_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for r in reflections {
            *topic_counts.entry(r.topic.clone()).or_insert(0) += 1;
        }
        for (topic, count) in &topic_counts {
            if *count >= 3 {
                patterns.push(format!(
                    "Recurring reflection topic '{}' ({} times) — consider systemic attention",
                    topic, count
                ));
            }
        }

        // Detect declining confidence
        if reflections.len() >= 3 {
            let recent: Vec<f64> = reflections.iter().rev().take(3).map(|r| r.confidence).collect();
            if recent.len() == 3 && recent[0] < recent[1] && recent[1] < recent[2] {
                patterns.push(
                    "Declining reflection confidence over last 3 reflections — \
                     memory quality may be degrading"
                        .to_string(),
                );
            }
        }

        // Detect unplanned actions (reflections with planned actions that were never reported)
        let unplanned: usize = reflections
            .iter()
            .filter(|r| !r.planned_actions.is_empty() && r.outcome.is_none())
            .count();
        if unplanned >= 5 {
            patterns.push(format!(
                "{} reflections have unexecuted planned actions — \
                 consider running a consolidation cycle",
                unplanned
            ));
        }

        patterns
    }

    /// Retrieve a reflection by ID from SQLite.
    pub fn get_reflection(&self, id: &str) -> Option<Reflection> {
        self.store.get_reflection(id).ok().flatten()
    }

    /// List recent reflections from SQLite.
    pub fn list_reflections(&self, limit: usize) -> Vec<Reflection> {
        self.store.list_reflections(limit).unwrap_or_default()
    }

    /// Delete a reflection from SQLite.
    pub fn delete_reflection(&self, id: &str) -> bool {
        self.store.delete_reflection(id).unwrap_or(false)
    }

    /// Retrieve a self-assessment by ID from SQLite.
    pub fn get_assessment(&self, id: &str) -> Option<SelfAssessment> {
        self.store.get_assessment(id).ok().flatten()
    }

    /// List recent self-assessments from SQLite.
    pub fn list_assessments(&self, limit: usize) -> Vec<SelfAssessment> {
        self.store.list_assessments(limit).unwrap_or_default()
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

    fn setup() -> ReflectionEngine {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        ReflectionEngine::new(store)
    }

    #[test]
    fn test_reflect() {
        let engine = setup();
        let reflection = engine.reflect(
            "memory quality",
            "Looking at my current state...",
            "Things look good",
            vec!["continue".to_string()],
            0.8,
            vec!["self-assessment".to_string()],
        );
        assert_eq!(reflection.topic, "memory quality");
        assert!(reflection.confidence > 0.5);
        assert!(!reflection.planned_actions.is_empty());
    }

    #[test]
    fn test_self_assess() {
        let engine = setup();
        let assessment = engine.self_assess();
        // Empty store should have low quality
        assert!(assessment.memory_quality_score < 0.5);
        assert!(!assessment.issues_detected.is_empty());
    }

    #[test]
    fn test_reflexion_loop() {
        let engine = setup();
        let (reflection, _assessment) = engine.reflexion_loop(
            "system health",
            "Starting a new session",
        );
        assert_eq!(reflection.topic, "system health");
        assert!(reflection.confidence >= 0.0);
    }

    #[test]
    fn test_analyze_patterns_empty() {
        let engine = setup();
        let patterns = engine.analyze_patterns(&[]);
        assert!(patterns.is_empty());
    }
}
