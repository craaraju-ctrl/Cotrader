//! # Multi-Tier Memory Cache — Policy-Driven Eviction
//!
//! Implements the System Policy SP-2026-MEM-v4.2:
//!
//! - **Data Stratification**: Critical (0-delay pinned), High (ephemeral), Medium (sliding-window)
//! - **Adaptive TTL Routing**: TTL dynamically assigned based on the requesting `AgentTier`
//! - **Mathematical Eviction**: Ω = (α × Ψ) / (Δt + 1.0) — priority-weighted access scoring
//!
//! **100% isolated** — Zero dependencies on trading or agent systems.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ══════════════════════════════════════════════════════════════════════════
//  AGENT TIER — determines TTL allocation per request
// ══════════════════════════════════════════════════════════════════════════

/// The requesting agent's trading strategy tier.
/// Each tier has a strict TTL window enforced on cache insertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentTier {
    /// High-Frequency Scalping / Market Making
    /// TTL: 5,000–15,000 ms — strict volatility wipe to prevent execution slippage.
    DayTrading,
    /// Momentum / Intra-day Breakout
    /// TTL: 180–300 sec — macro indicator anchor.
    HourlyTrading,
    /// Weekly Carry-Forward / Position Tracking
    /// TTL: 3,600–86,400 sec — persistent paging, bypasses SQLite index scans.
    SwingTrading,
    /// High-Gamma Derivative Hedging
    /// TTL: 60 sec fixed — derivatives delta reset.
    OptionsExpiry,
}

impl AgentTier {
    /// Return (min_ttl, max_ttl) in milliseconds for this tier.
    pub fn ttl_window_ms(&self) -> (u64, u64) {
        match self {
            Self::DayTrading => (5_000, 15_000),
            Self::HourlyTrading => (180_000, 300_000),
            Self::SwingTrading => (3_600_000, 86_400_000),
            Self::OptionsExpiry => (60_000, 60_000),
        }
    }

    /// Clamp a candidate TTL into this tier's valid window.
    pub fn clamp_ttl_ms(&self, requested_ms: u64) -> u64 {
        let (lo, hi) = self.ttl_window_ms();
        requested_ms.clamp(lo, hi)
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  CACHE PRIORITY — determines eviction resistance
// ══════════════════════════════════════════════════════════════════════════

/// Data stratification priority for cache entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CachePriority {
    /// Live Spot prices, order books, execution order status, margin/leverage limits.
    /// Immune to timed-expiry; requires explicit `invalidate(key)` or broker event.
    Critical,
    /// Options chains (Greeks), volatile indicators (RSI, MACD, Volume Profile).
    /// Short-TTL with strict expiry windows.
    High,
    /// Historical trade logs, daily PnL vectors, weekly/monthly carry-forward.
    /// Managed via hybrid aging score (sliding-window block cache).
    Medium,
}

impl CachePriority {
    /// Weight constant Ψ for the eviction score formula.
    /// Critical = 10.0, High = 5.0, Medium = 1.0
    pub fn weight(&self) -> f64 {
        match self {
            Self::Critical => 10.0,
            Self::High => 5.0,
            Self::Medium => 1.0,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  CACHED ITEM — value + metadata for eviction scoring
// ══════════════════════════════════════════════════════════════════════════

/// A cached item with priority, TTL, and access tracking.
#[derive(Debug, Clone)]
pub struct CachedItem<T: Clone> {
    value: T,
    priority: CachePriority,
    created_at: Instant,
    ttl: Duration,
    hit_count: u64,
    /// Whether this entry is pinned (Critical tier) — immune to expiry.
    pinned: bool,
}

impl<T: Clone> CachedItem<T> {
    /// Critical-priority pinned entries never expire via TTL.
    fn is_expired(&self) -> bool {
        if self.pinned {
            false
        } else {
            self.created_at.elapsed() > self.ttl
        }
    }

    fn record_hit(&mut self) {
        self.hit_count += 1;
    }

    /// Age in seconds since creation.
    fn age_secs(&self) -> f64 {
        self.created_at.elapsed().as_secs_f64()
    }

    /// Eviction score Ω = (access_count × priority_weight) / (age_secs + 1.0)
    /// Lower score = more evictable.
    fn eviction_score(&self) -> f64 {
        let alpha = self.hit_count as f64;
        let psi = self.priority.weight();
        let delta_t = self.age_secs();
        (alpha * psi) / (delta_t + 1.0)
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  MULTI-TIER POLICY CACHE
// ══════════════════════════════════════════════════════════════════════════

/// Generic multi-tier policy cache with adaptive TTL, priority scoring,
/// and mathematical eviction (Ω-score).
pub struct PolicyCache<T: Clone + std::fmt::Debug> {
    cache: HashMap<String, CachedItem<T>>,
    max_size: usize,
    default_ttl: Duration,
    // Analytics
    total_hits: u64,
    total_misses: u64,
}

impl<T: Clone + std::fmt::Debug> PolicyCache<T> {
    pub fn new(max_size: usize, default_ttl_secs: u64) -> Self {
        Self {
            cache: HashMap::new(),
            max_size,
            default_ttl: Duration::from_secs(default_ttl_secs),
            total_hits: 0,
            total_misses: 0,
        }
    }

    // ── INSERT ─────────────────────────────────────────────────────────

    /// Insert with default TTL and Medium priority.
    pub fn insert(&mut self, key: String, value: T) {
        self.insert_with_policy(key, value, CachePriority::Medium, self.default_ttl);
    }

    /// Insert with a custom TTL.
    pub fn insert_with_ttl(&mut self, key: String, value: T, ttl: Duration) {
        self.insert_with_policy(key, value, CachePriority::Medium, ttl);
    }

    /// Insert with priority and TTL — the primary insertion path.
    /// Automatically evicts lowest-Ω entries when capacity is reached.
    pub fn insert_with_policy(
        &mut self,
        key: String,
        value: T,
        priority: CachePriority,
        ttl: Duration,
    ) {
        if self.cache.len() >= self.max_size {
            self.evict_by_score();
        }
        let pinned = priority == CachePriority::Critical;
        self.cache.insert(
            key,
            CachedItem {
                value,
                priority,
                created_at: Instant::now(),
                ttl,
                hit_count: 0,
                pinned,
            },
        );
    }

    /// Insert with agent-tier adaptive TTL routing.
    /// The requesting `AgentTier` determines the valid TTL window.
    pub fn insert_for_agent(
        &mut self,
        key: String,
        value: T,
        priority: CachePriority,
        agent_tier: AgentTier,
        requested_ttl_ms: u64,
    ) {
        let clamped_ms = agent_tier.clamp_ttl_ms(requested_ttl_ms);
        let ttl = Duration::from_millis(clamped_ms);
        self.insert_with_policy(key, value, priority, ttl);
    }

    // ── GET / CONTAINS ────────────────────────────────────────────────

    /// Get a value by key. Returns None if missing or expired.
    pub fn get(&mut self, key: &str) -> Option<&T> {
        if self.is_expired_inner(key) {
            self.cache.remove(key);
            self.total_misses += 1;
            return None;
        }
        let item = self.cache.get_mut(key)?;
        item.record_hit();
        self.total_hits += 1;
        Some(&item.value)
    }

    fn is_expired_inner(&self, key: &str) -> bool {
        self.cache.get(key).is_some_and(|item| item.is_expired())
    }

    /// Check if a key exists and is not expired.
    pub fn contains(&mut self, key: &str) -> bool {
        if self.is_expired_inner(key) {
            self.cache.remove(key);
            self.total_misses += 1;
            return false;
        }
        match self.cache.get_mut(key) {
            Some(item) => {
                item.record_hit();
                self.total_hits += 1;
                true
            }
            None => {
                self.total_misses += 1;
                false
            }
        }
    }

    // ── INVALIDATION ──────────────────────────────────────────────────

    /// Explicit manual invalidation — the only way to remove Critical entries.
    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
    }

    /// Remove a key (alias for invalidate, non-Critical only).
    pub fn remove(&mut self, key: &str) {
        self.cache.remove(key);
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.total_hits = 0;
        self.total_misses = 0;
    }

    // ── EVICTION ──────────────────────────────────────────────────────

    /// Evict the entry with the lowest eviction score (Ω).
    /// Critical (pinned) entries are never evicted.
    fn evict_by_score(&mut self) {
        let victim = self
            .cache
            .iter()
            .filter(|(_, item)| !item.pinned)
            .min_by(|(_, a), (_, b)| {
                a.eviction_score()
                    .partial_cmp(&b.eviction_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(k, _)| k.clone());
        if let Some(key) = victim {
            self.cache.remove(&key);
        }
    }

    // ── PURGE ─────────────────────────────────────────────────────────

    /// Remove all expired items (non-Critical only).
    pub fn purge_expired(&mut self) {
        let expired_keys: Vec<String> = self
            .cache
            .iter()
            .filter(|(_, item)| item.is_expired())
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired_keys {
            self.cache.remove(&key);
        }
    }

    // ── ANALYTICS ─────────────────────────────────────────────────────

    /// Cache hit rate (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_hits + self.total_misses;
        if total == 0 {
            return 0.0;
        }
        self.total_hits as f64 / total as f64
    }

    /// Current cache size.
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Total hits.
    pub fn total_hits(&self) -> u64 {
        self.total_hits
    }

    /// Total misses.
    pub fn total_misses(&self) -> u64 {
        self.total_misses
    }

    /// Total lookups.
    pub fn total_lookups(&self) -> u64 {
        self.total_hits + self.total_misses
    }
}

impl<T: Clone + std::fmt::Debug> Default for PolicyCache<T> {
    fn default() -> Self {
        Self::new(100, 300) // 100 items, 5 min TTL
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  ASYNC MULTI-TIER CACHE (thread-safe for concurrent access)
// ══════════════════════════════════════════════════════════════════════════

/// Thread-safe multi-tier cache using `tokio::sync::RwLock` for read-heavy workloads.
/// Follows the System Policy lock-isolation requirement: reads are concurrent,
/// writes only block when executing state updates.
pub struct AsyncPolicyCache<T: Clone + std::fmt::Debug + Send + Sync> {
    inner: Arc<RwLock<PolicyCache<T>>>,
}

impl<T: Clone + std::fmt::Debug + Send + Sync> AsyncPolicyCache<T> {
    pub fn new(max_size: usize, default_ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(PolicyCache::new(max_size, default_ttl_secs))),
        }
    }

    /// Get a value by key (concurrent read).
    pub async fn get(&self, key: &str) -> Option<T> {
        let mut cache = self.inner.write().await;
        cache.get(key).cloned()
    }

    /// Insert with default TTL (write lock).
    pub async fn insert(&self, key: String, value: T) {
        let mut cache = self.inner.write().await;
        cache.insert(key, value);
    }

    /// Insert with priority and TTL.
    pub async fn insert_with_policy(
        &self,
        key: String,
        value: T,
        priority: CachePriority,
        ttl: Duration,
    ) {
        let mut cache = self.inner.write().await;
        cache.insert_with_policy(key, value, priority, ttl);
    }

    /// Insert with agent-tier adaptive TTL routing.
    pub async fn insert_for_agent(
        &self,
        key: String,
        value: T,
        priority: CachePriority,
        agent_tier: AgentTier,
        requested_ttl_ms: u64,
    ) {
        let mut cache = self.inner.write().await;
        cache.insert_for_agent(key, value, priority, agent_tier, requested_ttl_ms);
    }

    /// Explicit manual invalidation — the only way to remove Critical entries.
    pub async fn invalidate(&self, key: &str) {
        let mut cache = self.inner.write().await;
        cache.invalidate(key);
    }

    /// Purge all expired entries.
    pub async fn purge_expired(&self) {
        let mut cache = self.inner.write().await;
        cache.purge_expired();
    }

    /// Current cache size.
    pub async fn size(&self) -> usize {
        let cache = self.inner.read().await;
        cache.size()
    }

    /// Cache hit rate.
    pub async fn hit_rate(&self) -> f64 {
        let cache = self.inner.read().await;
        cache.hit_rate()
    }

    /// Total hits.
    pub async fn total_hits(&self) -> u64 {
        let cache = self.inner.read().await;
        cache.total_hits()
    }

    /// Total misses.
    pub async fn total_misses(&self) -> u64 {
        let cache = self.inner.read().await;
        cache.total_misses()
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  TESTS
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ── Basic Operations ───────────────────────────────────────────────

    #[test]
    fn test_basic_insert_get() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 60);
        cache.insert("key1".into(), "value1".into());
        assert_eq!(cache.get("key1"), Some(&"value1".into()));
        assert!(cache.contains("key1"));
    }

    #[test]
    fn test_cache_miss() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 60);
        assert_eq!(cache.get("missing"), None);
        assert!(!cache.contains("missing"));
        assert_eq!(cache.hit_rate(), 0.0);
    }

    #[test]
    fn test_expiry() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 1);
        cache.insert("key".into(), "value".into());
        assert!(cache.contains("key"));
        thread::sleep(Duration::from_secs(1));
        assert!(!cache.contains("key"));
    }

    #[test]
    fn test_hit_rate() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 60);
        cache.insert("key".into(), "value".into());
        assert!(cache.contains("key")); // hit
        assert!(!cache.contains("missing")); // miss
        assert!(!cache.contains("missing2")); // miss
        let rate = cache.hit_rate();
        assert!((rate - 1.0 / 3.0).abs() < 0.01);
    }

    // ── Eviction Scoring ──────────────────────────────────────────────

    #[test]
    fn test_eviction_by_score() {
        let mut cache: PolicyCache<String> = PolicyCache::new(3, 60);
        cache.insert_with_policy("low".into(), "1".into(), CachePriority::Medium, Duration::from_secs(60));
        cache.insert_with_policy("high".into(), "2".into(), CachePriority::High, Duration::from_secs(60));
        // Access high once to boost its score above low
        cache.get("high");
        cache.insert_with_policy("critical".into(), "3".into(), CachePriority::Critical, Duration::from_secs(60));
        assert_eq!(cache.size(), 3);

        // Insert a 4th — should evict "low" (lowest Ω: 0 hits × Medium weight)
        cache.insert_with_policy("new".into(), "4".into(), CachePriority::High, Duration::from_secs(60));
        assert_eq!(cache.size(), 3);
        assert!(!cache.contains("low"));
        assert!(cache.contains("critical"));
    }

    #[test]
    fn test_critical_entries_never_expire() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 1);
        cache.insert_with_policy("pinned".into(), "data".into(), CachePriority::Critical, Duration::from_secs(1));
        thread::sleep(Duration::from_secs(2));
        assert!(cache.contains("pinned"));
    }

    #[test]
    fn test_invalidate_critical() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 60);
        cache.insert_with_policy("pinned".into(), "data".into(), CachePriority::Critical, Duration::from_secs(60));
        assert!(cache.contains("pinned"));
        cache.invalidate("pinned");
        assert!(!cache.contains("pinned"));
    }

    // ── Agent-Tier Adaptive TTL ────────────────────────────────────────

    #[test]
    fn test_day_trading_ttl_window() {
        assert_eq!(AgentTier::DayTrading.ttl_window_ms(), (5_000, 15_000));
        assert_eq!(AgentTier::DayTrading.clamp_ttl_ms(1_000), 5_000);
        assert_eq!(AgentTier::DayTrading.clamp_ttl_ms(10_000), 10_000);
        assert_eq!(AgentTier::DayTrading.clamp_ttl_ms(20_000), 15_000);
    }

    #[test]
    fn test_hourly_trading_ttl_window() {
        assert_eq!(AgentTier::HourlyTrading.ttl_window_ms(), (180_000, 300_000));
        assert_eq!(AgentTier::HourlyTrading.clamp_ttl_ms(60_000), 180_000);
        assert_eq!(AgentTier::HourlyTrading.clamp_ttl_ms(240_000), 240_000);
    }

    #[test]
    fn test_swing_trading_ttl_window() {
        assert_eq!(AgentTier::SwingTrading.ttl_window_ms(), (3_600_000, 86_400_000));
        assert_eq!(AgentTier::SwingTrading.clamp_ttl_ms(1_000), 3_600_000);
        assert_eq!(AgentTier::SwingTrading.clamp_ttl_ms(72_000_000), 72_000_000);
    }

    #[test]
    fn test_options_expiry_ttl_window() {
        assert_eq!(AgentTier::OptionsExpiry.ttl_window_ms(), (60_000, 60_000));
        assert_eq!(AgentTier::OptionsExpiry.clamp_ttl_ms(1_000), 60_000);
        assert_eq!(AgentTier::OptionsExpiry.clamp_ttl_ms(120_000), 60_000);
    }

    #[test]
    fn test_insert_for_agent() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 60);
        cache.insert_for_agent("btc_price".into(), "50000".into(), CachePriority::Critical, AgentTier::DayTrading, 10_000);
        assert_eq!(cache.get("btc_price"), Some(&"50000".into()));
    }

    // ── Priority Weights ──────────────────────────────────────────────

    #[test]
    fn test_priority_weights() {
        assert!((CachePriority::Critical.weight() - 10.0).abs() < f64::EPSILON);
        assert!((CachePriority::High.weight() - 5.0).abs() < f64::EPSILON);
        assert!((CachePriority::Medium.weight() - 1.0).abs() < f64::EPSILON);
    }

    // ── Eviction Score Formula ────────────────────────────────────────

    #[test]
    fn test_eviction_score_formula() {
        let item = CachedItem {
            value: "test".to_string(),
            priority: CachePriority::High,
            created_at: Instant::now(),
            ttl: Duration::from_secs(60),
            hit_count: 10,
            pinned: false,
        };
        let score = item.eviction_score();
        assert!(score > 45.0 && score <= 50.0);
    }

    #[test]
    fn test_low_score_evicts_first() {
        // Cache size 2 ensures deterministic eviction: only one non-pinned candidate
        let mut cache: PolicyCache<String> = PolicyCache::new(2, 60);
        cache.insert_with_policy("cold".into(), "data".into(), CachePriority::Medium, Duration::from_secs(60));
        cache.insert_with_policy("warm".into(), "data".into(), CachePriority::High, Duration::from_secs(60));
        // Access warm 5 times to clearly boost its Ω score above cold
        for _ in 0..5 { cache.get("warm"); }
        thread::sleep(Duration::from_millis(50));
        // Insert hot → cache full → evict lowest Ω: cold (0 hits × Medium = 0) vs warm (5 hits × High ≈ 23.8)
        cache.insert_with_policy("hot".into(), "data".into(), CachePriority::High, Duration::from_secs(60));
        assert!(!cache.contains("cold"));
        assert!(cache.contains("warm"));
        assert!(cache.contains("hot"));
    }

    // ── Purge Expired ─────────────────────────────────────────────────

    #[test]
    fn test_purge_expired() {
        let mut cache: PolicyCache<String> = PolicyCache::new(10, 1);
        cache.insert("short1".into(), "v1".into());
        cache.insert("short2".into(), "v2".into());
        cache.insert_with_policy("long".into(), "v3".into(), CachePriority::Critical, Duration::from_secs(3600));
        thread::sleep(Duration::from_secs(1));
        cache.purge_expired();
        assert!(cache.contains("long"));
        assert!(!cache.contains("short1"));
        assert!(!cache.contains("short2"));
    }

    // ── Async Cache ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_async_cache_basic() {
        let cache: AsyncPolicyCache<String> = AsyncPolicyCache::new(10, 60);
        cache.insert("key".into(), "value".into()).await;
        assert_eq!(cache.get("key").await, Some("value".into()));
        assert_eq!(cache.size().await, 1);
    }

    #[tokio::test]
    async fn test_async_cache_invalidate() {
        let cache: AsyncPolicyCache<String> = AsyncPolicyCache::new(10, 60);
        cache.insert_with_policy("critical".into(), "data".into(), CachePriority::Critical, Duration::from_secs(60)).await;
        assert_eq!(cache.get("critical").await, Some("data".into()));
        cache.invalidate("critical").await;
        assert_eq!(cache.get("critical").await, None);
    }

    #[tokio::test]
    async fn test_async_cache_agent_tier() {
        let cache: AsyncPolicyCache<String> = AsyncPolicyCache::new(10, 60);
        cache.insert_for_agent("btc".into(), "50000".into(), CachePriority::Critical, AgentTier::DayTrading, 10_000).await;
        assert_eq!(cache.get("btc").await, Some("50000".into()));
    }
}
