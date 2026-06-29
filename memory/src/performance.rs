//! # Performance Optimizations Module
//!
//! Drop-in replacements for hot-path operations.
//! These wrap existing functionality with concurrent-friendly alternatives.

use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use tokio::sync::RwLock;

/// Thread-safe policy cache using DashMap for lock-free concurrent access.
/// Wraps the existing PolicyCache with concurrent-safe operations.
pub struct ConcurrentPolicyCache<T: Clone + std::fmt::Debug + Send + Sync> {
    entries: Arc<DashMap<String, CacheEntry<T>>>,
    max_size: usize,
    default_ttl: Duration,
    total_hits: Arc<std::sync::atomic::AtomicU64>,
    total_misses: Arc<std::sync::atomic::AtomicU64>,
}

#[derive(Debug, Clone)]
struct CacheEntry<T: Clone> {
    value: T,
    created_at: Instant,
    ttl: Duration,
}

impl<T: Clone + std::fmt::Debug + Send + Sync> ConcurrentPolicyCache<T> {
    pub fn new(max_size: usize, default_ttl_secs: u64) -> Self {
        Self {
            entries: Arc::new(DashMap::with_capacity(max_size)),
            max_size,
            default_ttl: Duration::from_secs(default_ttl_secs),
            total_hits: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_misses: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Get a cached value (lock-free read).
    pub fn get(&self, key: &str) -> Option<T> {
        let entry = self.entries.get(key)?;
        if entry.value().created_at.elapsed() > entry.value().ttl {
            drop(entry);
            self.entries.remove(key);
            self.total_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return None;
        }
        let value = entry.value().value.clone();
        self.total_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(value)
    }

    /// Insert a value (lock-free write).
    pub fn insert(&self, key: String, value: T) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// Insert with custom TTL.
    pub fn insert_with_ttl(&self, key: String, value: T, ttl: Duration) {
        if self.entries.len() >= self.max_size {
            self.evict_oldest();
        }
        self.entries.insert(key, CacheEntry {
            value,
            created_at: Instant::now(),
            ttl,
        });
    }

    /// Remove a cached value.
    pub fn remove(&self, key: &str) -> Option<T> {
        self.entries.remove(key).map(|(_, entry)| entry.value)
    }

    /// Check if key exists and is not expired.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.get(key).map_or(false, |entry| {
            entry.value().created_at.elapsed() <= entry.value().ttl
        })
    }

    /// Get cache hit rate.
    pub fn hit_rate(&self) -> f64 {
        let hits = self.total_hits.load(std::sync::atomic::Ordering::Relaxed);
        let misses = self.total_misses.load(std::sync::atomic::Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 { 0.0 } else { hits as f64 / total as f64 }
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.entries.len(),
            max_size: self.max_size,
            total_hits: self.total_hits.load(std::sync::atomic::Ordering::Relaxed),
            total_misses: self.total_misses.load(std::sync::atomic::Ordering::Relaxed),
            hit_rate: self.hit_rate(),
        }
    }

    /// Remove all expired entries.
    pub fn purge_expired(&self) {
        self.entries.retain(|_, entry| entry.created_at.elapsed() <= entry.ttl);
    }

    /// Clear all entries.
    pub fn clear(&self) {
        self.entries.clear();
    }

    fn evict_oldest(&self) {
        if let Some(oldest_key) = self.entries.iter()
            .min_by_key(|entry| entry.value().created_at)
            .map(|entry| entry.key().clone())
        {
            self.entries.remove(&oldest_key);
        }
    }
}

impl<T: Clone + std::fmt::Debug + Send + Sync> Clone for ConcurrentPolicyCache<T> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
            max_size: self.max_size,
            default_ttl: self.default_ttl,
            total_hits: Arc::clone(&self.total_hits),
            total_misses: Arc::clone(&self.total_misses),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub max_size: usize,
    pub total_hits: u64,
    pub total_misses: u64,
    pub hit_rate: f64,
}

/// Thread-safe store wrapper using RwLock for concurrent access.
/// Wraps MemoryStore with async-friendly locking.
pub struct ConcurrentStore<S> {
    inner: Arc<RwLock<S>>,
}

impl<S: Clone + Send + Sync> ConcurrentStore<S> {
    pub fn new(store: S) -> Self {
        Self {
            inner: Arc::new(RwLock::new(store)),
        }
    }

    /// Read access (multiple concurrent readers allowed).
    pub async fn read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&S) -> R,
    {
        let guard = self.inner.read().await;
        f(&guard)
    }

    /// Write access (exclusive).
    pub async fn write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut S) -> R,
    {
        let mut guard = self.inner.write().await;
        f(&mut guard)
    }
}

impl<S: Clone + Send + Sync> Clone for ConcurrentStore<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
