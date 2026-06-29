# Agentic Memory — Engineering Upgrades & Research Roadmap
## IMPLEMENTATION STATUS

| Upgrade | Status | Notes |
|---------|--------|-------|
| SQLite Connection Pool | ⏳ Deferred | Version conflict with sqlx (rusqlite 0.31 vs 0.35). Requires workspace-wide rusqlite upgrade. |
| DashMap Policy Cache | ✅ Implemented | `performance.rs` module with `ConcurrentPolicyCache<T>` — lock-free concurrent access via DashMap. |
| SIMD Hamming Distance | 📋 Planned | Requires `packed_simd` or `std::simd` (nightly). Documented in Part 1.C. |
| FinancialRegretScorer | 📋 Planned | Documented in Part 2.A with full code skeleton. |
| TradingRelation Enum | 📋 Planned | Documented in Part 2.B with full code skeleton. |
| Adaptive Ebbinghaus | 📋 Planned | Documented in Part 3.A — requires volatility feed integration. |
| Backtest Validation | 📋 Planned | Documented in Part 3.B — requires NATS bus + backtest runner. |
| Conflict Arbitrator | 📋 Planned | Documented in Part 3.C — game-theoretic namespace resolution. |

### What Was Implemented

**`memory/src/performance.rs`** — New module with:

1. **`ConcurrentPolicyCache<T>`** — Drop-in replacement for `PolicyCache`:
   - Uses `DashMap<String, CacheEntry<T>>` for lock-free concurrent access
   - Atomic counters for hit/miss tracking (no mutex needed)
   - Thread-safe `get()`, `insert()`, `remove()`, `contains()`
   - Background-safe `purge_expired()` and `evict_oldest()`
   - Clone-safe via `Arc` wrapping

2. **`ConcurrentStore<S>`** — Generic wrapper for any store:
   - Uses `tokio::sync::RwLock` for async-friendly locking
   - Multiple concurrent readers, exclusive writers
   - `read()` and `write()` methods with closures

### Build Status
- ✅ `cargo check -p agentic-memory` passes
- ✅ `cargo build --release -p agentic-memory` succeeds
- ✅ DashMap dependency added to Cargo.toml
- ✅ Module registered in lib.rs


*Based on complete code audit of store.rs, cache.rs, temporal.rs, graph.rs, consolidation.rs*

---

## PART 1: Core Code Upgrades

### A. Thread-Safety & Async Contention — SQLite Connection Pool

**File:** `memory/src/store.rs`

**Current State:**
```rust
pub(crate) conn: Arc<Mutex<Connection>>
```

Single synchronous mutex blocks all concurrent access. Tokio async tasks block on OS-level mutex.

**Required Change:**
```rust
pub(crate) pool: Arc<Pool<SqliteConnectionManager>>
```

**Implementation:**

1. Add to `memory/Cargo.toml`:
```toml
r2d2 = "0.8"
r2d2_sqlite = "0.28"
```

2. Replace `lock_db()` with pooled connection:
```rust
pub fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, r2d2::Error> {
    self.pool.get()
}
```

3. Pool configuration:
```rust
let pool = Pool::builder()
    .max_size(8)        // 8 concurrent connections
    .min_idle(Some(2))  // Keep 2 warm connections
    .build(manager)?;
```

4. Apply SQLite pragmas via connection hook:
```rust
let manager = manager.new_connection_hook(|conn| {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;
         PRAGMA temp_store=MEMORY;"
    ).expect("Failed to set pragmas");
});
```

**Impact:** 8x concurrency improvement. Background operations no longer block main trading thread.

---

### B. Hot-Path Cache — DashMap for PolicyCache

**File:** `memory/src/cache.rs`

**Current State:**
```rust
pub struct PolicyCache {
    entries: HashMap<String, CachedItem>,  // No synchronization
    // ...
}
```

**Required Change:**
```rust
pub struct PolicyCache {
    entries: DashMap<String, CachedItem>,  // Lock-free concurrent access
    // ...
}
```

**Implementation:**

1. Add to `memory/Cargo.toml`:
```toml
dashmap = "5"
```

2. Replace HashMap with DashMap:
```rust
use dashmap::DashMap;

pub struct PolicyCache {
    entries: DashMap<String, CachedItem>,
    max_size: usize,
    default_ttl: Duration,
}

impl PolicyCache {
    pub fn get(&self, key: &str) -> Option<CachedItem> {
        let entry = self.entries.get(key)?;
        if entry.is_expired() {
            drop(entry);
            self.entries.remove(key);
            return None;
        }
        Some(entry.value().clone())
    }

    pub fn insert(&self, key: String, item: CachedItem) {
        if self.entries.len() >= self.max_size {
            self.purge_expired();
        }
        self.entries.insert(key, item);
    }

    pub fn purge_expired(&self) {
        self.entries.retain(|_, item| !item.is_expired());
    }
}
```

3. Offload purge to background task:
```rust
pub async fn start_background_purge(cache: Arc<PolicyCache>) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        cache.purge_expired();
    }
}
```

**Impact:** Zero-lock reads. Market data thread never pays eviction cost.

---

### C. Binary Quantization — SIMD HNSW Index

**File:** `memory/src/vector.rs`

**Current State:**
```rust
pub fn hamming_distance(a: &[bool], b: &[bool]) -> f64 {
    let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    differing as f64 / a.len() as f64
}
```

Iterative bit comparison, no hardware acceleration.

**Required Change:** Add SIMD-accelerated Hamming distance using AVX2 intrinsics.

**Implementation:**

1. Add to `memory/Cargo.toml`:
```toml
packed_simd = "0.3"  # Or use std::simd when stabilized
```

2. SIMD Hamming distance:
```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// AVX2-accelerated Hamming distance for binary vectors.
/// Processes 256 bits per iteration.
#[cfg(target_arch = "x86_64")]
unsafe fn hamming_distance_simd(a: &[u64], b: &[u64]) -> f64 {
    let mut count = 0u32;
    let chunks = a.len() / 4;  // 4 x u64 = 256 bits

    for i in 0..chunks {
        let va = _mm256_loadu_si256(a.as_ptr().add(i * 4) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(i * 4) as *const __m256i);
        let xor = _mm256_xor_si256(va, vb);
        count += _mm256_popcnt_epi64(xor) as u32;
    }

    count as f64 / (a.len() * 64) as f64
}

#[cfg(not(target_arch = "x86_64"))]
fn hamming_distance_simd(a: &[u64], b: &[u64]) -> f64 {
    // Fallback for non-x86_64
    hamming_distance_scalar(a, b)
}
```

3. HNSW index structure:
```rust
pub struct HnswIndex {
    layers: Vec<Vec<usize>>,           // Layer connections
    vectors: Vec<Vec<u64>>,            // Binary quantized as u64 array
    dimension: usize,
    max_connections: usize,            // M parameter
    ef_construction: usize,            // Build-time search width
}

impl HnswIndex {
    pub fn search(&self, query: &[u64], k: usize) -> Vec<(usize, f64)> {
        // HNSW search with SIMD distance at each level
        // Returns (record_id, similarity_score)
    }
}
```

**Impact:** 10-30x faster binary search via hardware acceleration.

---

## PART 2: Domain-Specific Upgrades

### D. Financial Importance Scoring

**File:** `memory/src/consolidation.rs`

**Current State:**
```rust
pub fn score(&self, context: &ImportanceContext) -> f64 {
    let mut score = 0.0;
    score += (context.access_count as f64).min(100.0) / 100.0 * 0.3;
    score += recency_factor * 0.25;
    // ... generic text-based scoring
}
```

**Required:** FinancialRegretScorer that binds memory importance to actual portfolio impact.

**Implementation:**

```rust
use std::collections::HashMap;

/// Financial-aware importance scorer.
/// Binds memory longevity to portfolio drawdowns and profits.
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

#[derive(Debug, Clone)]
pub struct FinancialContext {
    /// Regret score from 0.0 (good) to 1.0 (bad)
    pub regret_score: f64,
    /// Absolute change in portfolio balance
    pub balance_delta: f64,
    /// Position size at time of event
    pub position_size: f64,
    /// Market regime (trending, ranging, volatile)
    pub regime: String,
    /// Whether this was a winning trade
    pub is_win: bool,
    /// Leverage used
    pub leverage: u32,
}

impl FinancialRegretScorer {
    /// Compute importance score for a financial memory record.
    /// Higher scores = more important to retain.
    pub fn score(&self, ctx: &FinancialContext) -> f64 {
        let mut score = 0.0;

        // Regret component: high regret = high importance
        // A loss of $1000 with regret 0.8 should score higher than routine $10 trade
        score += ctx.regret_score * self.regret_weight;

        // Balance delta: log-scale impact
        // log10(1000) = 3.0, log10(10) = 1.0
        let delta_impact = if ctx.balance_delta.abs() > 0.0 {
            ctx.balance_delta.abs().log10().clamp(0.0, 5.0) / 5.0
        } else {
            0.0
        };
        score += delta_impact * self.balance_delta_weight;

        // Position size: larger positions = more memorable
        let position_impact = (ctx.position_size / 100000.0).clamp(0.0, 1.0);
        score += position_impact * self.position_weight;

        // Regime significance: volatile regimes are more memorable
        let regime_impact = match ctx.regime.as_str() {
            "Volatile" | "HighVolatility" => 1.0,
            "TrendingBull" | "TrendingBear" => 0.7,
            "Ranging" => 0.4,
            _ => 0.3,
        };
        score += regime_impact * self.regret_weight;

        // Leverage amplifier: high leverage trades are more critical
        if ctx.leverage > 10 {
            score *= 1.0 + (ctx.leverage as f64 / 100.0);
        }

        // Win/loss asymmetry: losses are remembered more strongly
        if !ctx.is_win {
            score *= 1.2;
        }

        score.clamp(0.0, 1.0)
    }

    /// Score from metadata map (for backward compatibility)
    pub fn score_from_metadata(&self, metadata: &HashMap<String, String>) -> f64 {
        let regret = metadata.get("regret_score")
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

        self.score(&FinancialContext {
            regret_score: regret,
            balance_delta,
            position_size,
            regime,
            is_win,
            leverage,
        })
    }
}
```

**Impact:** Memories tied to actual portfolio impact persist longer. Routine events decay faster.

---

### E. Graph Relationship Taxonomy

**File:** `memory/src/graph.rs` + `memory/src/types.rs`

**Current State:**
```rust
pub struct GraphEdge {
    pub edge_id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,  // Generic string
    pub weight: f64,
}
```

**Required:** Structured trading relationship enums with domain-specific weights.

**Implementation:**

```rust
/// Trading-specific graph relationship types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TradingRelation {
    // Price relationships
    CorrelatedWith,        // BTC → ETH (positive correlation)
    InverselyCorrelated,   // BTC → Gold (negative correlation)
    Leads,                 // BTC leads alts by N minutes

    // Regime relationships
    RegimeChangeTo,        // Trending → Ranging
    ValidatedBy,           // Pattern confirmed by volume
    InvalidatedBy,         // Setup broken by news event

    // Signal relationships
    ConflictsWith,         // Bull signal vs Bear signal
    Strengthens,           // Multiple indicators align
    Weakens,               // Contradictory signals

    // Risk relationships
    HedgedBy,              // Position hedged by another
    ExposedTo,             // Portfolio exposure to factor
    LiquidatedAt,          // Position liquidation trigger

    // Memory relationships
    DerivedFrom,           // Lesson derived from trade
    Supersedes,            // New rule overrides old
    SimilarTo,             // Pattern match to historical
}

impl TradingRelation {
    /// Domain-specific weight multiplier for graph boosting.
    /// Higher weight = stronger influence on retrieval.
    pub fn boost_weight(&self) -> f64 {
        match self {
            // Strong negative relationships (evict unsafe params)
            Self::InvalidatedBy => -0.5,
            Self::ConflictsWith => -0.3,
            Self::Weakens => -0.1,

            // Strong positive relationships
            Self::ValidatedBy => 0.4,
            Self::Strengthens => 0.3,
            Self::Supersedes => 0.2,

            // Neutral informational
            Self::CorrelatedWith => 0.15,
            Self::InverselyCorrelated => 0.1,
            Self::Leads => 0.1,
            Self::DerivedFrom => 0.1,
            Self::SimilarTo => 0.1,

            // Risk-related (always important)
            Self::HedgedBy => 0.2,
            Self::ExposedTo => 0.25,
            Self::LiquidatedAt => 0.3,

            // Regime
            Self::RegimeChangeTo => 0.2,
        }
    }

    /// Convert to string for storage
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

    /// Parse from string
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
```

**Updated GraphBoost:**
```rust
fn boost_with_graph_reasoning(
    &self,
    results: &mut [SearchResult],
    boost_weight: f64,
) -> rusqlite::Result<()> {
    for result in results.iter_mut() {
        let edges = self.store.get_edges(&result.record.id)?;

        for edge in &edges {
            if let Some(relation) = TradingRelation::from_str(&edge.relation_type) {
                let domain_boost = relation.boost_weight();
                // Apply domain-specific weight instead of generic boost
                result.score += domain_boost * edge.weight;
            }
        }

        result.score = result.score.clamp(0.0, 1.0);
    }
    Ok(())
}
```

**Impact:** Graph traversal now applies domain-aware scoring. Invalidated relationships actively suppress retrieval.

---

## PART 3: Research & R&D Roadmap

### F. Adaptive Ebbinghaus Forgetting Curves

**File:** `memory/src/temporal.rs`

**Current State:**
```rust
pub fn compute_decay(&self, fact: &TemporalFact) -> f64 {
    let age_hours = self.age_hours(fact);
    (-self.decay_config.lambda * age_hours).exp()
}
```

Static λ regardless of market conditions.

**Research Objective:** Modulate λ based on real-time volatility.

**Proposed Model:**
```rust
/// Adaptive decay that responds to market volatility.
pub fn compute_adaptive_decay(
    &self,
    fact: &TemporalFact,
    current_volatility: f64,  // e.g., 30-day HV or ATR%
    fact_regime: &str,         // regime when fact was created
) -> f64 {
    let base_lambda = self.decay_config.lambda;
    let age_hours = self.age_hours(fact);

    // Volatility-adjusted lambda
    // High vol → faster decay (assumptions broken faster)
    // Low vol → slower decay (stable environment)
    let vol_multiplier = match current_volatility {
        v if v > 0.05 => 3.0,   // >5% vol: 3x faster decay
        v if v > 0.03 => 2.0,   // 3-5% vol: 2x faster
        v if v > 0.01 => 1.0,   // 1-3% vol: normal
        _ => 0.5,               // <1% vol: half decay (very stable)
    };

    // Regime change detection: if current regime differs from fact's regime,
    // apply additional decay acceleration
    let regime_penalty = if fact_regime != self.current_regime {
        2.0  // Regime changed: 2x additional decay
    } else {
        1.0
    };

    let adaptive_lambda = base_lambda * vol_multiplier * regime_penalty;

    // Structural rules have a floor (never fully decay)
    let min_decay = if fact.content_type == "procedure" || fact.content_type == "rule" {
        0.3  // Procedures retain at least 30% strength
    } else {
        0.0
    };

    ((-adaptive_lambda * age_hours).exp()).max(min_decay)
}
```

**Integration points:**
- Pass `current_volatility` from RAT's `MarketMetricsMeter`
- Pass `current_regime` from `RegimeDetector`
- Store `fact_regime` in `temporal_facts.metadata_json`

---

### G. Procedural Backtest Validation Loop

**File:** `memory/src/evolution.rs`

**Current State:**
```rust
fn distill_procedural(&mut self) -> Result<Vec<String>, String> {
    // Immediately promotes high-importance records to procedural
    // No validation against historical data
}
```

**Research Objective:** Validate procedural rules against backtest before promotion.

**Proposed Design:**
```rust
async fn distill_with_validation(&mut self) -> Result<Vec<String>, String> {
    let candidates = self.find_distillation_candidates()?;

    let mut validated = Vec::new();

    for candidate in candidates {
        // 1. Extract the proposed rule
        let rule = extract_rule_from_record(&candidate)?;

        // 2. Send to backtest via EventBus
        let backtest_request = BacktestRequest {
            rule: rule.clone(),
            symbol: "BTC".to_string(),
            lookback_days: 90,
            min_trades: 20,
        };

        let result = self.event_bus.request_backtest(backtest_request).await?;

        // 3. Validate positive expectancy
        if result.profit_factor > 1.2
            && result.sharpe_ratio > 1.5
            && result.max_drawdown < 0.15
            && result.win_rate > 0.45
        {
            // 4. Promote to procedural memory
            self.store.insert_into_tier(
                &candidate.record,
                MemoryTier::Procedural,
                0.95,
                None,
                None,
            )?;
            validated.push(candidate.record.id);
        } else {
            // 5. Mark as rejected, update confidence
            tracing::info!(
                "Rule rejected: PF={:.2} Sharpe={:.2} DD={:.2}",
                result.profit_factor,
                result.sharpe_ratio,
                result.max_drawdown
            );
        }
    }

    Ok(validated)
}
```

**Integration points:**
- `rat-core/src/backtest.rs` already has backtest engine
- Connect via `rat-eventbus` subject `rat.backtest.request`
- Store validation results in `evolution_events` table

---

### H. Cross-Namespace Adversarial Conflict Resolution

**File:** `memory/src/consolidation.rs`

**Current State:**
```rust
pub fn detect_conflicts(&self, records: &[TieredRecord]) -> Vec<Conflict> {
    // Simple content similarity check
    // No resolution mechanism
}
```

**Research Objective:** Game-theoretic arbitration between conflicting agent namespaces.

**Proposed Design:**
```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ConflictArbitrator {
    /// Historical accuracy per namespace
    namespace_accuracy: HashMap<String, f64>,
    /// Historical variance in predictions
    namespace_variance: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct ArbitratedConflict {
    pub conflict: Conflict,
    pub winner_namespace: String,
    pub confidence: f64,
    pub reasoning: String,
}

impl ConflictArbitrator {
    /// Resolve conflict using game-theoretic model.
    /// Prioritizes namespace with lowest prediction variance.
    pub fn resolve(&self, conflict: &Conflict) -> ArbitratedConflict {
        let ns_a = &conflict.namespace_a;
        let ns_b = &conflict.namespace_b;

        // Variance-weighted scoring
        // Lower variance = more reliable = higher weight
        let var_a = self.namespace_variance.get(ns_a).copied().unwrap_or(1.0);
        let var_b = self.namespace_variance.get(ns_b).copied().unwrap_or(1.0);

        let weight_a = 1.0 / (1.0 + var_a);
        let weight_b = 1.0 / (1.0 + var_b);

        // Accuracy-weighted scoring
        let acc_a = self.namespace_accuracy.get(ns_a).copied().unwrap_or(0.5);
        let acc_b = self.namespace_accuracy.get(ns_b).copied().unwrap_or(0.5);

        // Combined score: accuracy / variance (Sharpe-like)
        let score_a = acc_a * weight_a;
        let score_b = acc_b * weight_b;

        let (winner, confidence) = if score_a > score_b {
            (ns_a.clone(), score_a / (score_a + score_b))
        } else {
            (ns_b.clone(), score_b / (score_a + score_b))
        };

        ArbitratedConflict {
            conflict: conflict.clone(),
            winner_namespace: winner,
            confidence,
            reasoning: format!(
                "Resolved by variance-weighted accuracy: {} (score={:.3}) > {} (score={:.3})",
                ns_a, score_a, ns_b, score_b
            ),
        }
    }

    /// Update accuracy after outcome is known
    pub fn record_outcome(&mut self, namespace: &str, was_correct: bool) {
        let acc = self.namespace_accuracy.entry(namespace.to_string()).or_insert(0.5);
        // Exponential moving average
        *acc = *acc * 0.9 + (if was_correct { 1.0 } else { 0.0 }) * 0.1;
    }

    /// Update variance after prediction
    pub fn record_prediction(&mut self, namespace: &str, predicted: f64, actual: f64) {
        let var = self.namespace_variance.entry(namespace.to_string()).or_insert(0.0);
        let error = (predicted - actual).abs();
        // Exponential moving average of squared error
        *var = *var * 0.9 + error * error * 0.1;
    }
}
```

**Integration points:**
- Track per-namespace metrics in `evolution_events` table
- Call `record_outcome()` after each trade closes
- Call `resolve()` when `detect_conflicts()` finds multi-namespace conflicts
- Store arbitration decisions in `reflections` table

---

## PART 4: Implementation Priority

| Priority | Upgrade | Effort | Impact | Risk |
|----------|---------|--------|--------|------|
| P0 | SQLite connection pool | 2 hours | Critical | Low |
| P0 | DashMap policy cache | 1 hour | High | Low |
| P1 | FinancialRegretScorer | 3 hours | High | Medium |
| P1 | TradingRelation enum | 2 hours | High | Low |
| P2 | SIMD Hamming distance | 4 hours | Medium | Medium |
| P2 | Adaptive Ebbinghaus | 8 hours | High | High |
| P3 | Backtest validation loop | 6 hours | High | High |
| P3 | Conflict arbitrator | 8 hours | Medium | High |

**Recommended order:** P0 → P1 → P2 → P3

---

*End of Upgrades Document*
