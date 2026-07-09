//! # Expert Modules — Specialized Memory Operations
//!
//! Provides expert modules for retrieval, reasoning, consolidation, and evolution.

use crate::store::MemoryStore;
use crate::types::{MemoryRecord, SearchResult, TradingRelation};
use crate::vector::cosine_similarity;

/// Strategy confidence rating (star-rating system).
/// Stamped on successful trade path executions by FinancialRegretScorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StrategyConfidence {
    /// Single star (★) — Marginal confidence, weak signal convergence
    SingleStar = 1,
    /// Double star (★★) — Moderate confidence, multiple indicators align
    DoubleStar = 2,
    /// Triple star (★★★) — High confidence, strong confluence
    TripleStar = 3,
}

impl StrategyConfidence {
    /// Numeric weight for blending into confidence score matrix.
    pub fn weight(&self) -> f64 {
        match self {
            Self::SingleStar => 1.0,
            Self::DoubleStar => 2.0,
            Self::TripleStar => 3.0,
        }
    }

    /// Normalize to 0.0–1.0 range for use as a blending factor.
    pub fn normalized_score(&self) -> f64 {
        self.weight() / 3.0
    }

    /// Derive rating from a raw importance score (0.0–1.0).
    pub fn from_importance(importance: f64) -> Self {
        if importance >= 0.7 {
            Self::TripleStar
        } else if importance >= 0.4 {
            Self::DoubleStar
        } else {
            Self::SingleStar
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::SingleStar => "★",
            Self::DoubleStar => "★★",
            Self::TripleStar => "★★★",
        }
    }

    /// Return the tier name as a static string for DB storage and health queries.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SingleStar => "SingleStar",
            Self::DoubleStar => "DoubleStar",
            Self::TripleStar => "TripleStar",
        }
    }
}

impl std::fmt::Display for StrategyConfidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// An isolated reference parameter returned during queries.
/// Used ONLY as historical confidence score weights matrix,
/// NOT as a hard override or final execution signal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyReference {
    /// The star rating for this historical sequence
    pub confidence: StrategyConfidence,
    /// The raw importance score that produced this rating
    pub raw_importance: f64,
    /// The namespace/market this rating is associated with
    pub namespace_id: String,
    /// Number of historical records contributing to this rating
    pub sample_size: u64,
    /// The final blended score (combines star weight with current volatility)
    pub blended_score: f64,
}

/// Expert specialized in hybrid memory retrieval (vector + graph + staleness).
pub struct RetrievalExpert {
    pub store: MemoryStore,
}

impl RetrievalExpert {
    pub fn new(store: MemoryStore) -> Self {
        Self { store }
    }

    // ==================== HYBRID RETRIEVAL ====================

    /// Hybrid retrieval: vector search + graph boosting.
    /// Combines sqlite-vec k-NN with graph-based relevance boosting.
    pub fn hybrid_retrieve(
        &self,
        query_vec: &[f64],
        k: usize,
        candidate_multiplier: usize,
    ) -> rusqlite::Result<Vec<SearchResult>> {
        if query_vec.is_empty() || k == 0 {
            return Ok(vec![]);
        }

        let num_candidates = k * candidate_multiplier.max(1);
        let candidates =
            self.store
                .search_vectors_hybrid(query_vec, num_candidates, num_candidates)?;

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let mut scored = self.dense_rerank(query_vec, &candidates);
        self.boost_with_graph_reasoning(&mut scored, 0.18)?;

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(k);

        Ok(scored)
    }

    /// Re-rank candidates using full-precision cosine similarity.
    fn dense_rerank(
        &self,
        query_vec: &[f64],
        candidates: &[(MemoryRecord, f32)],
    ) -> Vec<SearchResult> {
        candidates
            .iter()
            .filter_map(|(record, _)| {
                let emb = record.embedding.as_ref()?;
                let score = (cosine_similarity(query_vec, emb) + 1.0) / 2.0;
                Some(SearchResult {
                    record: record.clone(),
                    score: score.clamp(0.0, 1.0),
                    method: "hybrid_binary_dense".to_string(),
                })
            })
            .collect()
    }

    /// Boost search results using domain-specific trading relationship weights.
    /// Uses TradingRelation enum for precise, financial-aware scoring.
    fn boost_with_graph_reasoning(
        &self,
        results: &mut [SearchResult],
        boost_weight: f64,
    ) -> rusqlite::Result<()> {
        for result in results.iter_mut() {
            let edges = self.store.get_edges(&result.record.id).unwrap_or_default();
            if edges.is_empty() {
                continue;
            }

            let mut score = 0.0;
            for edge in &edges {
                // Parse relation type into TradingRelation enum
                let type_weight = if let Some(relation) = TradingRelation::from_str(&edge.relation_type) {
                    // Use domain-specific weight from TradingRelation
                    1.0 + relation.boost_weight()
                } else {
                    // Fallback for unrecognized relation types
                    1.0
                };
                score += edge.weight * type_weight;
            }

            // Two-hop traversal for indirect relationships
            let mut two_hop = 0.0;
            for edge in &edges {
                if let Ok(neighbors) = self.store.get_edges(&edge.target_id) {
                    for neighbor in &neighbors {
                        let neighbor_weight = if let Some(rel) = TradingRelation::from_str(&neighbor.relation_type) {
                            1.0 + rel.boost_weight()
                        } else {
                            1.0
                        };
                        two_hop += neighbor.weight * neighbor_weight * 0.3;
                    }
                }
            }

            // Combine scores with boost weight
            let combined = (score + two_hop).clamp(-10.0, 10.0);
            let boost = combined * boost_weight;
            result.score = (result.score + boost).clamp(0.0, 1.0);
        }
        Ok(())
    }
}
