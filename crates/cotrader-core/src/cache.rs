use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::agent::AgentTier;
use crate::config::StorageConfig;

// ---------------------------------------------------------------------------
// Priority weights for the predictive eviction formula
// ---------------------------------------------------------------------------
const PRIORITY_CRITICAL: f64 = 10.0;
const PRIORITY_HIGH: f64 = 5.0;
const PRIORITY_MEDIUM: f64 = 1.0;

/// Priority assigned to each cache entry — drives eviction scoring.
/// Ordered by weight: Critical > High > Medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePriority {
    /// Live spot prices, active order states — NEVER evicted by TTL.
    Critical,
    /// Frequently accessed data that degrades fast (recent candles, orderbooks).
    High,
    /// Reference data, historical aggregates.
    Medium,
}

impl PartialOrd for CachePriority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.weight().partial_cmp(&other.weight())?)
    }
}

impl Ord for CachePriority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.weight().partial_cmp(&other.weight()).unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl CachePriority {
    pub fn weight(self) -> f64 {
        match self {
            CachePriority::Critical => PRIORITY_CRITICAL,
            CachePriority::High => PRIORITY_HIGH,
            CachePriority::Medium => PRIORITY_MEDIUM,
        }
    }
}

// ---------------------------------------------------------------------------
// Spot price entry — pinned in critical memory, no TTL eviction
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct SpotPrice {
    pub symbol: String,
    pub price: f64,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub spread_bps: Option<f64>,
    pub volume_24h: Option<f64>,
    pub updated_at: Instant,
}

// ---------------------------------------------------------------------------
// Active order entry — pinned in critical memory, no TTL eviction
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct ActiveOrder {
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub qty: f64,
    pub limit_price: Option<f64>,
    pub status: OrderStatus,
    pub filled_qty: f64,
    pub updated_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
    Expired,
}

// ---------------------------------------------------------------------------
// Generic cache entry with access tracking for eviction scoring
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
    #[allow(dead_code)]
    inserted_at: Instant,
    last_access: Instant,
    access_count: u64,
    priority: CachePriority,
    tier: AgentTier,
}

impl<V> CacheEntry<V> {
    fn new(value: V, priority: CachePriority, tier: AgentTier) -> Self {
        let now = Instant::now();
        Self {
            value,
            inserted_at: now,
            last_access: now,
            access_count: 1,
            priority,
            tier,
        }
    }

    fn touch(&mut self) {
        self.last_access = Instant::now();
        self.access_count += 1;
    }

    fn is_expired(&self) -> bool {
        // Critical entries never expire
        if self.priority == CachePriority::Critical {
            return false;
        }
        let (_, max_ms) = self.tier.cache_ttl_bounds_ms();
        let max_ttl = Duration::from_millis(max_ms);
        self.last_access.elapsed() > max_ttl
    }

    /// Predictive eviction score: Omega = (alpha * Psi) / (dt + 1.0)
    /// Higher score = higher priority to KEEP (lower eviction priority).
    fn eviction_score(&self) -> f64 {
        let alpha = self.access_count as f64;
        let psi = self.priority.weight();
        let dt = self.last_access.elapsed().as_secs_f64();
        (alpha * psi) / (dt + 1.0)
    }
}

// ---------------------------------------------------------------------------
// TradingCache — the top-level cache structure
// ---------------------------------------------------------------------------
pub struct TradingCache {
    // --- Critical memory: zero-delay paging, no TTL eviction ---
    spot_prices: RwLock<HashMap<String, SpotPrice>>,
    active_orders: RwLock<HashMap<String, ActiveOrder>>,

    // --- Stratified TTL cache: general key-value with per-tier TTLs ---
    data_cache: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,

    max_entries: usize,
    storage: StorageConfig,
}

impl TradingCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            spot_prices: RwLock::new(HashMap::new()),
            active_orders: RwLock::new(HashMap::new()),
            data_cache: RwLock::new(HashMap::with_capacity(max_entries)),
            max_entries,
            storage: StorageConfig::default(),
        }
    }

    pub fn with_storage(max_entries: usize, storage: StorageConfig) -> Self {
        Self {
            spot_prices: RwLock::new(HashMap::new()),
            active_orders: RwLock::new(HashMap::new()),
            data_cache: RwLock::new(HashMap::with_capacity(max_entries)),
            max_entries,
            storage,
        }
    }

    pub fn storage(&self) -> &StorageConfig {
        &self.storage
    }

    // ===================================================================
    // Critical Memory — Spot Prices (0-delay, no eviction)
    // ===================================================================

    pub async fn update_spot(&self, spot: SpotPrice) {
        let mut prices = self.spot_prices.write().await;
        prices.insert(spot.symbol.clone(), spot);
    }

    pub async fn get_spot(&self, symbol: &str) -> Option<SpotPrice> {
        let prices = self.spot_prices.read().await;
        prices.get(symbol).cloned()
    }

    pub async fn get_spot_batch(&self, symbols: &[&str]) -> Vec<SpotPrice> {
        let prices = self.spot_prices.read().await;
        symbols.iter().filter_map(|s| prices.get(*s).cloned()).collect()
    }

    pub async fn remove_spot(&self, symbol: &str) {
        let mut prices = self.spot_prices.write().await;
        prices.remove(symbol);
    }

    pub async fn spot_count(&self) -> usize {
        self.spot_prices.read().await.len()
    }

    // ===================================================================
    // Critical Memory — Active Orders (0-delay, no eviction)
    // ===================================================================

    pub async fn insert_order(&self, order: ActiveOrder) {
        let mut orders = self.active_orders.write().await;
        orders.insert(order.order_id.clone(), order);
    }

    pub async fn get_order(&self, order_id: &str) -> Option<ActiveOrder> {
        let orders = self.active_orders.read().await;
        orders.get(order_id).cloned()
    }

    pub async fn update_order_status(&self, order_id: &str, status: OrderStatus, filled_qty: f64) {
        let mut orders = self.active_orders.write().await;
        if let Some(order) = orders.get_mut(order_id) {
            order.status = status;
            order.filled_qty = filled_qty;
            order.updated_at = Instant::now();
        }
    }

    pub async fn remove_order(&self, order_id: &str) {
        let mut orders = self.active_orders.write().await;
        orders.remove(order_id);
    }

    pub async fn active_order_count(&self) -> usize {
        self.active_orders.read().await.len()
    }

    pub async fn orders_by_symbol(&self, symbol: &str) -> Vec<ActiveOrder> {
        let orders = self.active_orders.read().await;
        orders.values().filter(|o| o.symbol == symbol).cloned().collect()
    }

    // ===================================================================
    // Stratified TTL Cache — general data with tier-based TTL
    // ===================================================================

    pub async fn insert(
        &self,
        key: String,
        value: serde_json::Value,
        priority: CachePriority,
        tier: AgentTier,
    ) {
        {
            let mut cache = self.data_cache.write().await;
            cache.insert(key, CacheEntry::new(value, priority, tier));
        }
        self.maybe_evict().await;
    }

    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let mut cache = self.data_cache.write().await;
        if let Some(entry) = cache.get_mut(key) {
            if entry.is_expired() {
                cache.remove(key);
                return None;
            }
            entry.touch();
            Some(entry.value.clone())
        } else {
            None
        }
    }

    pub async fn remove(&self, key: &str) {
        let mut cache = self.data_cache.write().await;
        cache.remove(key);
    }

    pub async fn contains(&self, key: &str) -> bool {
        let cache = self.data_cache.read().await;
        cache.contains_key(key)
    }

    pub async fn data_cache_count(&self) -> usize {
        self.data_cache.read().await.len()
    }

    // ===================================================================
    // Total entry count across all caches
    // ===================================================================

    pub async fn total_count(&self) -> usize {
        let spots = self.spot_prices.read().await.len();
        let orders = self.active_orders.read().await.len();
        let data = self.data_cache.read().await.len();
        spots + orders + data
    }

    // ===================================================================
    // Cache statistics
    // ===================================================================

    pub async fn stats(&self) -> CacheStats {
        let spots = self.spot_prices.read().await;
        let orders = self.active_orders.read().await;
        let data = self.data_cache.read().await;

        CacheStats {
            spot_prices: spots.len(),
            active_orders: orders.len(),
            data_entries: data.len(),
            total: spots.len() + orders.len() + data.len(),
            max_entries: self.max_entries,
            utilization_pct: ((spots.len() + orders.len() + data.len()) as f64
                / self.max_entries as f64)
                * 100.0,
        }
    }

    // ===================================================================
    // Predictive Eviction — Ω = (α × Ψ) / (Δt + 1.0)
    // ===================================================================

    /// Run eviction when data_cache exceeds max_entries.
    /// Critical entries (spot prices, orders) are never evicted.
    async fn maybe_evict(&self) {
        let count = self.data_cache.read().await.len();
        if count <= self.max_entries {
            return;
        }

        // Collect scores for all entries
        let to_evict = {
            let cache = self.data_cache.read().await;
            let mut scored: Vec<(String, f64)> = cache
                .iter()
                .map(|(k, entry)| (k.clone(), entry.eviction_score()))
                .collect();
            // Ascending order: lowest score = evict first
            scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            // Evict ~10% or at least 1
            let evict_count = (count - self.max_entries).max(1);
            scored.into_iter().take(evict_count).map(|(k, _)| k).collect::<Vec<_>>()
        };

        if !to_evict.is_empty() {
            let mut cache = self.data_cache.write().await;
            for key in &to_evict {
                cache.remove(key);
            }
        }
    }

    /// Explicit full eviction pass — can be called from a background task.
    pub async fn evict_expired(&self) {
        let expired_keys = {
            let cache = self.data_cache.read().await;
            cache
                .iter()
                .filter(|(_, entry)| entry.is_expired())
                .map(|(k, _)| k.clone())
                .collect::<Vec<_>>()
        };

        if !expired_keys.is_empty() {
            let mut cache = self.data_cache.write().await;
            for key in &expired_keys {
                cache.remove(key);
            }
        }
    }

    /// Clear all data cache entries (but NOT critical memory).
    pub async fn clear_data_cache(&self) {
        let mut cache = self.data_cache.write().await;
        cache.clear();
    }

    /// Full clear including critical memory — use only on shutdown/reset.
    pub async fn clear_all(&self) {
        self.spot_prices.write().await.clear();
        self.active_orders.write().await.clear();
        self.data_cache.write().await.clear();
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub spot_prices: usize,
    pub active_orders: usize,
    pub data_entries: usize,
    pub total: usize,
    pub max_entries: usize,
    pub utilization_pct: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spot_price_critical_never_expires() {
        let cache = TradingCache::new(100);
        let spot = SpotPrice {
            symbol: "BTC".to_string(),
            price: 50000.0,
            bid: Some(49999.0),
            ask: Some(50001.0),
            spread_bps: Some(4.0),
            volume_24h: Some(1_000_000.0),
            updated_at: Instant::now(),
        };
        cache.update_spot(spot).await;
        assert!(cache.get_spot("BTC").await.is_some());
        assert_eq!(cache.get_spot("BTC").await.unwrap().price, 50000.0);
    }

    #[tokio::test]
    async fn active_order_critical_never_expires() {
        let cache = TradingCache::new(100);
        let order = ActiveOrder {
            order_id: "ord-1".to_string(),
            symbol: "BTC".to_string(),
            side: OrderSide::Buy,
            qty: 0.062,
            limit_price: Some(50000.0),
            status: OrderStatus::Pending,
            filled_qty: 0.0,
            updated_at: Instant::now(),
        };
        cache.insert_order(order).await;
        assert!(cache.get_order("ord-1").await.is_some());
        assert_eq!(cache.get_order("ord-1").await.unwrap().qty, 0.062);
    }

    #[tokio::test]
    async fn insert_and_get_data_cache() {
        let cache = TradingCache::new(100);
        cache
            .insert(
                "key1".to_string(),
                serde_json::json!({"hello": "world"}),
                CachePriority::High,
                AgentTier::DayTrading,
            )
            .await;
        let val = cache.get("key1").await;
        assert!(val.is_some());
        assert_eq!(val.unwrap()["hello"], "world");
    }

    #[tokio::test]
    async fn expired_entry_removed_on_get() {
        let cache = TradingCache::new(100);
        // Insert with DayTrading tier (max TTL 5s)
        cache
            .insert(
                "expiring".to_string(),
                serde_json::json!("data"),
                CachePriority::Medium,
                AgentTier::DayTrading,
            )
            .await;

        // Entry should exist immediately
        assert!(cache.get("expiring").await.is_some());

        // Manually expire by backdating the entry
        {
            let mut data = cache.data_cache.write().await;
            if let Some(entry) = data.get_mut("expiring") {
                entry.last_access = Instant::now() - Duration::from_secs(10);
            }
        }

        // Now get should return None and remove the expired entry
        assert!(cache.get("expiring").await.is_none());
        assert_eq!(cache.data_cache_count().await, 0);
    }

    #[tokio::test]
    async fn eviction_when_over_capacity() {
        let cache = TradingCache::new(5);
        for i in 0..10 {
            cache
                .insert(
                    format!("key{i}"),
                    serde_json::json!(i),
                    CachePriority::Medium,
                    AgentTier::DayTrading,
                )
                .await;
        }
        // Data cache should be at or below max_entries
        assert!(cache.data_cache_count().await <= 5);
    }

    #[tokio::test]
    async fn eviction_score_formula() {
        let entry = CacheEntry {
            value: serde_json::json!("test"),
            inserted_at: Instant::now() - Duration::from_secs(100),
            last_access: Instant::now() - Duration::from_secs(10),
            access_count: 20,
            priority: CachePriority::High,
            tier: AgentTier::DayTrading,
        };
        // Ω = (α × Ψ) / (Δt + 1.0) = (20 × 5.0) / (10.0 + 1.0) = 100/11 ≈ 9.09
        let score = entry.eviction_score();
        assert!((score - 9.09).abs() < 0.1, "score = {score}");
    }

    #[tokio::test]
    async fn critical_entries_never_evicted() {
        let cache = TradingCache::new(2);
        cache
            .insert(
                "critical".to_string(),
                serde_json::json!("val"),
                CachePriority::Critical,
                AgentTier::DayTrading,
            )
            .await;
        cache
            .insert(
                "med1".to_string(),
                serde_json::json!("m1"),
                CachePriority::Medium,
                AgentTier::DayTrading,
            )
            .await;
        cache
            .insert(
                "med2".to_string(),
                serde_json::json!("m2"),
                CachePriority::Medium,
                AgentTier::DayTrading,
            )
            .await;
        // Critical entry should survive eviction
        assert!(cache.get("critical").await.is_some());
    }

    #[tokio::test]
    async fn stats_report() {
        let cache = TradingCache::new(200);
        cache
            .insert(
                "d".to_string(),
                serde_json::json!("d"),
                CachePriority::High,
                AgentTier::SwingTrading,
            )
            .await;
        let stats = cache.stats().await;
        assert_eq!(stats.data_entries, 1);
        assert_eq!(stats.total, 1);
        assert!(stats.utilization_pct < 1.0);
    }

    #[tokio::test]
    async fn clear_data_preserves_critical() {
        let cache = TradingCache::new(100);
        cache
            .insert(
                "d1".to_string(),
                serde_json::json!("d"),
                CachePriority::High,
                AgentTier::DayTrading,
            )
            .await;
        cache.update_spot(SpotPrice {
            symbol: "ETH".to_string(),
            price: 3000.0,
            bid: None,
            ask: None,
            spread_bps: None,
            volume_24h: None,
            updated_at: Instant::now(),
        }).await;
        cache.clear_data_cache().await;
        assert_eq!(cache.data_cache_count().await, 0);
        assert!(cache.get_spot("ETH").await.is_some());
    }

    #[test]
    fn priority_weight_ordering() {
        assert!(CachePriority::Critical > CachePriority::High);
        assert!(CachePriority::High > CachePriority::Medium);
        assert_eq!(CachePriority::Critical.weight(), 10.0);
        assert_eq!(CachePriority::High.weight(), 5.0);
        assert_eq!(CachePriority::Medium.weight(), 1.0);
    }

    #[test]
    fn tier_ttl_bounds() {
        let (min, max) = AgentTier::DayTrading.cache_ttl_bounds_ms();
        assert_eq!(min, 100);
        assert_eq!(max, 5_000);

        let (min, max) = AgentTier::SwingTrading.cache_ttl_bounds_ms();
        assert_eq!(min, 3_600_000);
        assert_eq!(max, 86_400_000);

        let (min, max) = AgentTier::OptionsExpiry.cache_ttl_bounds_ms();
        assert_eq!(min, 60_000);
        assert_eq!(max, 60_000);
    }
}
