//! # Memory Types — Production-Grade Hierarchical Memory
//!
//! **100% domain-agnostic** — Zero dependencies on trading or any specific system.
//!
//! ## Memory Tier Architecture
//!
//! - `Working`   — Ephemeral context buffer, TTL-managed, auto-evicts
//! - `Episodic`  — Time-bound events and experiences
//! - `Semantic`  — Facts, preferences, deduplicated knowledge
//! - `Procedural`— Learned tools, workflows, behavioral rules
//!
//! ## Extended Capabilities
//!
//! - Bitemporal metadata (valid_time + transaction_time)
//! - Knowledge graph relationships with recursive traversal
//! - Chain-of-thought reasoning chains with confidence scoring
//! - Expert opinions from specialized modules
//! - Evolution events for self-adaptation tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Memory Record (unchanged, base type) ───────────────────────────────────

/// A generic memory record that can hold any type of content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub content_type: String,
    pub metadata: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f64>>,
    pub timestamp: String,
}

impl MemoryRecord {
    pub fn new(id: String, content: String, content_type: String) -> Self {
        Self {
            id,
            content,
            content_type,
            metadata: HashMap::new(),
            embedding: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_embedding(mut self, embedding: Vec<f64>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

// ── Memory Tier Enum ───────────────────────────────────────────────────────

/// The four tiers of the hierarchical memory system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryTier {
    /// Ephemeral context buffer — TTL-managed, auto-evicted, in-memory
    Working,
    /// Time-bound events and experiences — what happened
    Episodic,
    /// Facts, preferences, concepts — what is true
    Semantic,
    /// Learned tools, workflows, behavioral rules — how to act
    Procedural,
}

impl MemoryTier {
    /// All tiers in order from most volatile to most permanent.
    pub fn all() -> Vec<MemoryTier> {
        vec![
            MemoryTier::Working,
            MemoryTier::Episodic,
            MemoryTier::Semantic,
            MemoryTier::Procedural,
        ]
    }

    /// The next more permanent tier for promotion.
    pub fn promote_to(&self) -> Option<MemoryTier> {
        match self {
            MemoryTier::Working => Some(MemoryTier::Episodic),
            MemoryTier::Episodic => Some(MemoryTier::Semantic),
            MemoryTier::Semantic => Some(MemoryTier::Procedural),
            MemoryTier::Procedural => None,
        }
    }

    /// The next more volatile tier for demotion.
    pub fn demote_to(&self) -> Option<MemoryTier> {
        match self {
            MemoryTier::Working => None,
            MemoryTier::Episodic => Some(MemoryTier::Working),
            MemoryTier::Semantic => Some(MemoryTier::Episodic),
            MemoryTier::Procedural => Some(MemoryTier::Semantic),
        }
    }
}

impl std::fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryTier::Working => write!(f, "working"),
            MemoryTier::Episodic => write!(f, "episodic"),
            MemoryTier::Semantic => write!(f, "semantic"),
            MemoryTier::Procedural => write!(f, "procedural"),
        }
    }
}

impl std::str::FromStr for MemoryTier {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "working" => Ok(MemoryTier::Working),
            "episodic" => Ok(MemoryTier::Episodic),
            "semantic" => Ok(MemoryTier::Semantic),
            "procedural" => Ok(MemoryTier::Procedural),
            _ => Err(format!("Unknown memory tier: {}", s)),
        }
    }
}

// ── Tiered Record ───────────────────────────────────────────────────────────

/// A memory record with tier, importance, and access tracking metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredRecord {
    pub record: MemoryRecord,
    pub tier: MemoryTier,
    /// Importance score 0.0–1.0 (used for promotion/eviction decisions)
    pub importance: f64,
    /// How many times this record has been accessed
    pub access_count: u64,
    /// Last access timestamp (RFC 3339)
    pub last_accessed: String,
    /// TTL in seconds (None = permanent in that tier)
    pub ttl_seconds: Option<u64>,
    /// Parent record ID for hierarchical relationships
    pub parent_id: Option<String>,
    /// Free-form tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
}

impl TieredRecord {
    /// Convert this tiered record into a TemporalFact for decay calculations.
    /// Shared helper used by both evolution and consolidation engines.
    pub fn to_temporal_fact(&self) -> TemporalFact {
        TemporalFact {
            fact_id: self.record.id.clone(),
            content: self.record.content.clone(),
            content_type: self.record.content_type.clone(),
            valid_from: self.record.timestamp.clone(),
            valid_to: None,
            sys_start: self.record.timestamp.clone(),
            sys_end: None,
            version: 1,
            previous_version_id: None,
            decay_score: 1.0,
            recall_count: self.access_count,
            last_recalled: if self.last_accessed.is_empty() {
                None
            } else {
                Some(self.last_accessed.clone())
            },
            importance: self.importance,
            metadata: std::collections::HashMap::new(),
        }
    }
}

// ── Tier Configuration ─────────────────────────────────────────────────────

/// Configuration for a specific memory tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Maximum number of records in this tier
    pub max_records: usize,
    /// Default TTL in seconds (None = permanent)
    pub default_ttl_seconds: Option<u64>,
    /// Importance threshold for promotion to the next tier (0.0–1.0)
    pub promotion_threshold: f64,
    /// Importance threshold for demotion to the previous tier (0.0–1.0)
    pub demotion_threshold: f64,
    /// Whether auto-promotion is enabled for this tier
    pub auto_promote: bool,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            max_records: 1000,
            default_ttl_seconds: None,
            promotion_threshold: 0.7,
            demotion_threshold: 0.2,
            auto_promote: true,
        }
    }
}

impl TierConfig {
    /// Sensible defaults for each tier.
    pub fn for_tier(tier: MemoryTier) -> Self {
        match tier {
            MemoryTier::Working => Self {
                max_records: 100,
                default_ttl_seconds: Some(3600), // 1 hour
                promotion_threshold: 0.5,
                demotion_threshold: 0.1,
                auto_promote: true,
            },
            MemoryTier::Episodic => Self {
                max_records: 10_000,
                default_ttl_seconds: Some(86400 * 30), // 30 days
                promotion_threshold: 0.7,
                demotion_threshold: 0.2,
                auto_promote: true,
            },
            MemoryTier::Semantic => Self {
                max_records: 100_000,
                default_ttl_seconds: None, // permanent
                promotion_threshold: 0.85,
                demotion_threshold: 0.15,
                auto_promote: false, // manual or expert-driven
            },
            MemoryTier::Procedural => Self {
                max_records: 10_000,
                default_ttl_seconds: None, // permanent
                promotion_threshold: 0.95,
                demotion_threshold: 0.1,
                auto_promote: false,
            },
        }
    }
}

// ── Tier Statistics ────────────────────────────────────────────────────────

/// Statistics for a specific tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierStats {
    pub tier: MemoryTier,
    pub total_records: u64,
    pub total_with_embeddings: u64,
    pub average_importance: f64,
    pub total_accesses: u64,
    pub storage_bytes: u64,
    pub config: TierConfig,
}

/// Aggregated stats across all tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_records: u64,
    pub total_with_embeddings: u64,
    pub content_types: HashMap<String, u64>,
    pub storage_bytes: u64,
    pub tier_breakdown: HashMap<String, TierStats>,
}

// ── Knowledge Graph Types ──────────────────────────────────────────────────

/// Trading-specific graph relationship types.
/// Each variant has a domain-specific weight for retrieval boosting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TradingRelation {
    // Price relationships
    /// Positive correlation (e.g., BTC → ETH)
    CorrelatedWith,
    /// Negative correlation (e.g., BTC → Gold)
    InverselyCorrelated,
    /// One asset leads another by N minutes
    Leads,

    // Regime relationships
    /// Market regime transition
    RegimeChangeTo,
    /// Setup confirmed by evidence
    ValidatedBy,
    /// Setup broken by event
    InvalidatedBy,

    // Signal relationships
    /// Contradictory signals
    ConflictsWith,
    /// Multiple indicators align
    Strengthens,
    /// Contradictory indicators
    Weakens,

    // Risk relationships
    /// Position hedged by another
    HedgedBy,
    /// Portfolio exposure to factor
    ExposedTo,
    /// Position liquidation trigger
    LiquidatedAt,

    // Memory relationships
    /// Lesson derived from trade
    DerivedFrom,
    /// New rule overrides old
    Supersedes,
    /// Pattern match to historical
    SimilarTo,
}

impl TradingRelation {
    /// Domain-specific weight multiplier for graph boosting.
    /// Positive values boost retrieval, negative values suppress it.
    pub fn boost_weight(&self) -> f64 {
        match self {
            // Strong negative relationships (evict unsafe params)
            Self::InvalidatedBy => -0.50,
            Self::ConflictsWith => -0.30,
            Self::Weakens => -0.10,

            // Strong positive relationships
            Self::ValidatedBy => 0.40,
            Self::Strengthens => 0.30,
            Self::Supersedes => 0.20,

            // Risk-related (always important)
            Self::LiquidatedAt => 0.30,
            Self::ExposedTo => 0.25,
            Self::HedgedBy => 0.20,

            // Regime
            Self::RegimeChangeTo => 0.20,

            // Correlation
            Self::CorrelatedWith => 0.15,
            Self::InverselyCorrelated => 0.10,
            Self::Leads => 0.10,

            // Memory
            Self::DerivedFrom => 0.10,
            Self::SimilarTo => 0.10,
        }
    }

    /// Convert to string for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CorrelatedWith => "correlated_with",
            Self::InverselyCorrelated => "inversely_correlated",
            Self::Leads => "leads",
            Self::RegimeChangeTo => "regime_change_to",
            Self::ValidatedBy => "validated_by",
            Self::InvalidatedBy => "invalidated_by",
            Self::ConflictsWith => "conflicts_with",
            Self::Strengthens => "strengthens",
            Self::Weakens => "weakens",
            Self::HedgedBy => "hedged_by",
            Self::ExposedTo => "exposed_to",
            Self::LiquidatedAt => "liquidated_at",
            Self::DerivedFrom => "derived_from",
            Self::Supersedes => "supersedes",
            Self::SimilarTo => "similar_to",
        }
    }

    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "correlated_with" => Some(Self::CorrelatedWith),
            "inversely_correlated" => Some(Self::InverselyCorrelated),
            "leads" => Some(Self::Leads),
            "regime_change_to" => Some(Self::RegimeChangeTo),
            "validated_by" => Some(Self::ValidatedBy),
            "invalidated_by" => Some(Self::InvalidatedBy),
            "conflicts_with" => Some(Self::ConflictsWith),
            "strengthens" => Some(Self::Strengthens),
            "weakens" => Some(Self::Weakens),
            "hedged_by" => Some(Self::HedgedBy),
            "exposed_to" => Some(Self::ExposedTo),
            "liquidated_at" => Some(Self::LiquidatedAt),
            "derived_from" => Some(Self::DerivedFrom),
            "supersedes" => Some(Self::Supersedes),
            "similar_to" => Some(Self::SimilarTo),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub edge_id: String,
    pub source_id: String,
    pub target_id: String,
    /// Relationship type, e.g. "related_to", "causes", "depends_on", "part_of"
    pub relation_type: String,
    /// Edge weight 0.0–1.0 (strength of relationship)
    pub weight: f64,
    /// Arbitrary metadata
    pub metadata: HashMap<String, String>,
    pub created_at: String,
}

/// Result of a graph traversal step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphTraversalResult {
    pub node_id: String,
    pub depth: u32,
    pub path: Vec<String>,
    pub cumulative_weight: f64,
}

// ── Temporal / Bitemporal Metadata ─────────────────────────────────────────

/// Bitemporal metadata for tracking when a fact was true vs when it was recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalMetadata {
    /// When this fact was true in the real world (RFC 3339)
    pub valid_from: String,
    /// When this fact ceased being true (None = still valid)
    pub valid_to: Option<String>,
    /// When this fact was recorded in the system (RFC 3339)
    pub sys_start: String,
    /// When this record was superseded (None = current version)
    pub sys_end: Option<String>,
}

// ── Reasoning / Chain-of-Thought Types ─────────────────────────────────────

/// A single step in a chain of reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub step_index: u32,
    /// The premise or context for this step
    pub premise: String,
    /// The inference or action taken
    pub inference: String,
    /// The conclusion drawn from this step
    pub conclusion: String,
    /// Confidence in this step (0.0–1.0)
    pub confidence: f64,
    /// What tool or method was used
    pub tool_used: Option<String>,
    /// Whether this step succeeded
    pub success: bool,
    /// Timestamp of this step
    pub timestamp: String,
}

/// A complete chain of reasoning for a task or goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    pub chain_id: String,
    pub goal: String,
    pub steps: Vec<ReasoningStep>,
    pub final_conclusion: Option<String>,
    pub overall_confidence: f64,
    pub success: bool,
    /// Memory record IDs that were consulted during reasoning
    pub consulted_records: Vec<String>,
    /// Tags for retrieval
    pub tags: Vec<String>,
    pub created_at: String,
    pub duration_ms: u64,
}

// ── Expert Module Types ────────────────────────────────────────────────────

/// Types of expert modules available in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExpertType {
    /// Specialized in memory retrieval across all tiers
    Retrieval,
    /// Specialized in chain-of-thought reasoning
    Reasoning,
    /// Specialized in memory consolidation and management
    Consolidation,
    /// Specialized in system evolution and adaptation
    Evolution,
}

impl ExpertType {
    pub fn all() -> Vec<ExpertType> {
        vec![
            ExpertType::Retrieval,
            ExpertType::Reasoning,
            ExpertType::Consolidation,
            ExpertType::Evolution,
        ]
    }
}

impl std::fmt::Display for ExpertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpertType::Retrieval => write!(f, "retrieval"),
            ExpertType::Reasoning => write!(f, "reasoning"),
            ExpertType::Consolidation => write!(f, "consolidation"),
            ExpertType::Evolution => write!(f, "evolution"),
        }
    }
}

/// An opinion or recommendation from an expert module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertOpinion {
    pub opinion_id: String,
    pub expert_type: ExpertType,
    pub target_record_id: Option<String>,
    pub recommendation: String,
    pub reasoning: String,
    pub confidence: f64,
    pub action_taken: Option<String>,
    pub created_at: String,
}

// ── Consolidation Types ────────────────────────────────────────────────────

/// Result of a consolidation cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    pub cycle_id: String,
    pub started_at: String,
    pub completed_at: String,
    pub records_processed: u64,
    pub records_extracted: u64,
    pub records_deduplicated: u64,
    pub records_merged: u64,
    pub records_summarized: u64,
    pub records_promoted: u64,
    pub records_demoted: u64,
    pub records_evicted: u64,
    pub conflicts_detected: u64,
    pub conflicts_resolved: u64,
    pub insights_generated: Vec<String>,
    pub duration_ms: u64,
}

/// Context for computing importance scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceContext {
    pub access_count: u64,
    pub age_seconds: f64,
    pub has_embedding: bool,
    pub content_length: usize,
    pub content_type: String,
    pub tier: MemoryTier,
    pub graph_connections: usize,
    pub expert_endorsements: usize,
}

// ── Evolution Types ────────────────────────────────────────────────────────

/// An event recorded by the evolution system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionEvent {
    pub event_id: String,
    pub event_type: String, // e.g. "tier_tuned", "procedural_distilled", "stale_pruned"
    pub description: String,
    pub previous_value: Option<String>,
    pub new_value: Option<String>,
    pub confidence: f64,
    pub timestamp: String,
}

// ── Reflection Types ──────────────────────────────────────────────────────

/// A single reflection entry — the agent's inner monologue about its own state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflection {
    pub reflection_id: String,
    /// What the agent is reflecting on (e.g. "recent memory quality")
    pub topic: String,
    /// The agent's inner monologue text
    pub monologue: String,
    /// What the agent concluded from this reflection
    pub conclusion: String,
    /// Actions the agent recommends or plans to take
    #[serde(default)]
    pub planned_actions: Vec<String>,
    /// What actually happened after this reflection
    #[serde(default)]
    pub outcome: Option<String>,
    /// Confidence in the reflection's quality (0.0–1.0)
    pub confidence: f64,
    /// Tags for retrieval
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
}

/// A self-assessment of the memory system's overall health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfAssessment {
    pub assessment_id: String,
    pub memory_quality_score: f64,
    pub coherence_score: f64,
    pub staleness_score: f64,
    pub diversity_score: f64,
    pub overall_health: f64,
    pub issues_detected: Vec<String>,
    pub recommendations: Vec<String>,
    pub created_at: String,
}

// ── Temporal Memory Types ──────────────────────────────────────────────────

/// A fact with temporal versioning — tracks when it was true and when it was recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalFact {
    pub fact_id: String,
    pub content: String,
    pub content_type: String,
    /// When this fact became true (RFC 3339)
    pub valid_from: String,
    /// When this fact ceased being true (None = still valid)
    pub valid_to: Option<String>,
    /// When this record was first created in the system
    pub sys_start: String,
    /// When this version was superseded (None = current version)
    pub sys_end: Option<String>,
    /// Version number (incremented on updates)
    pub version: u32,
    /// Reference to the previous version (None = first version)
    pub previous_version_id: Option<String>,
    /// Ebbinghaus-based decay score (0.0–1.0, lower = more decayed)
    pub decay_score: f64,
    /// How many times this fact has been recalled
    pub recall_count: u64,
    /// Last time this fact was recalled (RFC 3339)
    pub last_recalled: Option<String>,
    /// Importance score (0.0–1.0)
    pub importance: f64,
    pub metadata: HashMap<String, String>,
}

/// Decay configuration following Ebbinghaus forgetting curve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayConfig {
    /// Half-life in days (default: 30 days)
    pub half_life_days: f64,
    /// Minimum recall boost (0.0–1.0, default: 0.3)
    pub min_recall_boost: f64,
    /// Decay acceleration factor (default: 1.0)
    pub acceleration: f64,
    /// Threshold below which a fact is considered stale (default: 0.1)
    pub stale_threshold: f64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            half_life_days: 30.0,
            min_recall_boost: 0.3,
            acceleration: 1.0,
            stale_threshold: 0.1,
        }
    }
}

// ── Context Management Types ───────────────────────────────────────────────

/// A context block that fits within a context window (MemGPT/Letta pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBlock {
    pub block_id: String,
    /// The block label (e.g. "persona", "user_preferences", "recent")
    pub label: String,
    /// The text content of this block
    pub content: String,
    /// Whether this block is pinned (always included in context)
    pub pinned: bool,
    /// Priority for eviction when context is full (lower = evicted first)
    pub priority: i32,
    /// Maximum tokens this block can hold
    pub max_tokens: usize,
    /// Current token count
    pub current_tokens: usize,
    /// When this block was last updated
    pub last_updated: String,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

/// Summary of archived/evicted context for recall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub summary_id: String,
    /// What was summarized
    pub topic: String,
    /// The summary text
    pub summary: String,
    /// Original block IDs that were summarized
    pub source_block_ids: Vec<String>,
    pub created_at: String,
}

/// Context window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Maximum total tokens in the context window
    pub max_tokens: usize,
    /// Token budget reserved for pinned blocks
    pub pinned_budget: usize,
    /// Whether to auto-summarize evicted blocks
    pub auto_summarize: bool,
    /// Maximum number of archived summaries to keep
    pub max_archived_summaries: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8192,
            pinned_budget: 2048,
            auto_summarize: true,
            max_archived_summaries: 100,
        }
    }
}

// ── Namespace Types (Multi-Agent Sharing) ──────────────────────────────────

/// A namespace for isolating memories across agents or users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub namespace_id: String,
    pub name: String,
    pub description: String,
    /// Who owns this namespace
    pub owner: String,
    /// Which namespaces this one can read from (inheritance)
    pub read_parents: Vec<String>,
    /// Which namespaces can read from this one
    pub write_children: Vec<String>,
    pub created_at: String,
}

/// Search Result (existing, kept unchanged).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub record: MemoryRecord,
    pub score: f64,
    pub method: String,
}

// ── Storage Configuration ──────────────────────────────────────────────────

/// Configuration for the storage backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub db_path: String,
    pub max_ram_entries: usize,
    pub auto_embed: bool,
    /// Dimension for vector embeddings (used by sqlite-vec vec0 tables)
    #[serde(default = "default_vector_dim")]
    pub vector_dimension: usize,
}

fn default_vector_dim() -> usize {
    768
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: ":memory:".into(),
            max_ram_entries: 100,
            auto_embed: false,
            vector_dimension: default_vector_dim(),
        }
    }
}

// ── Text Chunk ─────────────────────────────────────────────────────────────

/// A chunk of text with its position in the original document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
    pub index: usize,
    pub text: String,
    pub token_count: usize,
}
