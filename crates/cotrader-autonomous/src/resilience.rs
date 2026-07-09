//! # Resilience — Bulkhead Isolation & 3-Tier Circuit Breaker Hierarchy
//!
//! ## Bulkhead Pools
//! - `broker_api` — max 5 concurrent broker API calls
//! - `data_feed` — max 10 concurrent data feed connections
//! - `database_write` — max 3 concurrent DB writes
//! - `database_read` — max 20 concurrent DB reads
//! - `agent_evaluation` — max 8 concurrent agent evaluations
//!
//! ## Circuit Breaker Hierarchy (3-Tier)
//! - Tier 1: Per-symbol breakers (e.g., BTC-specific issues)
//! - Tier 2: Per-exchange breakers (e.g., Binance API outage)
//! - Tier 3: Global system breaker (memory pressure, consensus failure)

use crate::circuit_breaker::{BreakerState, CircuitBreaker, CircuitBreakerConfig};
use crate::types::{BulkheadFull, BulkheadName};
use crate::types::bulkhead_names;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};

// ═══════════════════════════════════════════════════════════════════════════════
// Bulkhead Pools
// ═══════════════════════════════════════════════════════════════════════════════

/// A single bulkhead pool — bounds concurrent operations of a class.
pub struct Bulkhead {
    name: BulkheadName,
    semaphore: Arc<Semaphore>,
    max_queue_ms: u64,
}

impl Bulkhead {
    /// Create a new bulkhead with the given max concurrent permits.
    pub fn new(name: BulkheadName, max_concurrent: usize, max_queue_ms: u64) -> Self {
        Self {
            name,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_queue_ms,
        }
    }

    /// Acquire a permit from this bulkhead.
    /// Returns an error if the semaphore cannot be acquired within `max_queue_ms`.
    pub async fn acquire(&self) -> Result<BulkheadPermit, BulkheadFull> {
        // Use try_acquire_owned on cloned Arc to get OwnedSemaphorePermit
        let owned = self.semaphore.clone();
        match owned.try_acquire_owned() {
            Ok(permit) => Ok(BulkheadPermit {
                _inner: BulkheadPermitInner::Owned(permit),
                name: self.name,
            }),
            Err(_) => {
                // Wait with timeout using acquire_owned
                match tokio::time::timeout(
                    Duration::from_millis(self.max_queue_ms),
                    self.semaphore.clone().acquire_owned(),
                )
                .await
                {
                    Ok(Ok(permit)) => Ok(BulkheadPermit {
                        _inner: BulkheadPermitInner::Owned(permit),
                        name: self.name,
                    }),
                    _ => Err(BulkheadFull {
                        name: self.name,
                        max_queue_ms: self.max_queue_ms,
                    }),
                }
            }
        }
    }

    /// Get the current number of available permits.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// A permit that releases the semaphore slot when dropped.
pub struct BulkheadPermit {
    _inner: BulkheadPermitInner,
    name: BulkheadName,
}

enum BulkheadPermitInner {
    Owned(tokio::sync::OwnedSemaphorePermit),
    Acquired(tokio::sync::SemaphorePermit<'static>),
}

impl BulkheadPermit {
    pub fn pool_name(&self) -> BulkheadName {
        self.name
    }
}

/// Registry of all standard bulkhead pools.
pub struct BulkheadRegistry {
    pools: HashMap<BulkheadName, Bulkhead>,
}

impl BulkheadRegistry {
    /// Create the standard 5-pool registry.
    pub fn new() -> Self {
        let mut pools = HashMap::new();
        pools.insert(
            bulkhead_names::BROKER_API,
            Bulkhead::new(bulkhead_names::BROKER_API, 5, 500),
        );
        pools.insert(
            bulkhead_names::DATA_FEED,
            Bulkhead::new(bulkhead_names::DATA_FEED, 10, 1000),
        );
        pools.insert(
            bulkhead_names::DATABASE_WRITE,
            Bulkhead::new(bulkhead_names::DATABASE_WRITE, 3, 2000),
        );
        pools.insert(
            bulkhead_names::DATABASE_READ,
            Bulkhead::new(bulkhead_names::DATABASE_READ, 20, 500),
        );
        pools.insert(
            bulkhead_names::AGENT_EVALUATION,
            Bulkhead::new(bulkhead_names::AGENT_EVALUATION, 8, 300),
        );
        Self { pools }
    }

    /// Acquire a permit from the named bulkhead pool.
    pub async fn acquire(&self, name: BulkheadName) -> Result<BulkheadPermit, BulkheadFull> {
        match self.pools.get(name) {
            Some(pool) => pool.acquire().await,
            None => Err(BulkheadFull {
                name,
                max_queue_ms: 0,
            }),
        }
    }

    /// Get available permits for a pool.
    pub fn available(&self, name: BulkheadName) -> usize {
        self.pools.get(name).map(|p| p.available_permits()).unwrap_or(0)
    }
}

impl Default for BulkheadRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3-Tier Circuit Breaker Hierarchy
// ═══════════════════════════════════════════════════════════════════════════════

/// The type of exchange ID for circuit breaker scoping.
pub type ExchangeId = String;

/// Severity of a failure — determines which tier(s) are tripped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FailureSeverity {
    /// Symbol-specific failure (e.g., illiquid market, data stall).
    SymbolSpecific,
    /// Exchange-specific failure (e.g., API outage, rate limit).
    ExchangeSpecific,
    /// Systemic failure (e.g., memory pressure, consensus failure).
    Systemic,
}

/// A recorded operation failure for circuit breaker evaluation.
#[derive(Debug, Clone)]
pub struct OperationFailure {
    pub symbol: String,
    pub exchange: ExchangeId,
    pub severity: FailureSeverity,
    pub reason: String,
}

/// 3-tier circuit breaker hierarchy.
///
/// Tier 1: Per-symbol breakers — protect against symbol-specific issues.
/// Tier 2: Per-exchange breakers — protect against exchange-specific issues.
/// Tier 3: Global system breaker — protect against systemic issues.
///
/// Trading requires ALL three tiers to be ARMED for the (symbol, exchange) pair.
pub struct CircuitBreakerHierarchy {
    /// Tier 1: Per-symbol circuit breakers.
    symbol_breakers: RwLock<HashMap<String, Arc<CircuitBreaker>>>,
    /// Tier 2: Per-exchange circuit breakers.
    exchange_breakers: RwLock<HashMap<ExchangeId, Arc<CircuitBreaker>>>,
    /// Tier 3: Global system circuit breaker.
    global_breaker: Arc<CircuitBreaker>,
    /// Default breaker config.
    config: CircuitBreakerConfig,
}

impl CircuitBreakerHierarchy {
    /// Create a new hierarchy with default config.
    pub fn new() -> Self {
        Self {
            symbol_breakers: RwLock::new(HashMap::new()),
            exchange_breakers: RwLock::new(HashMap::new()),
            global_breaker: Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default())),
            config: CircuitBreakerConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            symbol_breakers: RwLock::new(HashMap::new()),
            exchange_breakers: RwLock::new(HashMap::new()),
            global_breaker: Arc::new(CircuitBreaker::new(config.clone())),
            config,
        }
    }

    /// Get or create a per-symbol circuit breaker.
    pub async fn symbol_breaker(&self, symbol: &str) -> Arc<CircuitBreaker> {
        let mut breakers = self.symbol_breakers.write().await;
        breakers
            .entry(symbol.to_string())
            .or_insert_with(|| Arc::new(CircuitBreaker::new(self.config.clone())))
            .clone()
    }

    /// Get or create a per-exchange circuit breaker.
    pub async fn exchange_breaker(&self, exchange: &str) -> Arc<CircuitBreaker> {
        let mut breakers = self.exchange_breakers.write().await;
        breakers
            .entry(exchange.to_string())
            .or_insert_with(|| Arc::new(CircuitBreaker::new(self.config.clone())))
            .clone()
    }

    /// Get the global breaker reference.
    pub fn global_breaker_ref(&self) -> Arc<CircuitBreaker> {
        self.global_breaker.clone()
    }

    /// Check if trading is allowed for a (symbol, exchange) pair.
    /// ALL three tiers must be ARMED for trading to be allowed.
    pub async fn is_trading_allowed(&self, symbol: &str, exchange: &str) -> bool {
        // Tier 3: Global check first (fastest path — no lock contention on read)
        if !self.global_breaker.is_trading_allowed().await {
            return false;
        }

        // Tier 2: Exchange check
        {
            let breakers = self.exchange_breakers.read().await;
            if let Some(breaker) = breakers.get(exchange) {
                if !breaker.is_trading_allowed().await {
                    return false;
                }
            }
        }

        // Tier 1: Symbol check
        {
            let breakers = self.symbol_breakers.read().await;
            if let Some(breaker) = breakers.get(symbol) {
                if !breaker.is_trading_allowed().await {
                    return false;
                }
            }
        }

        true
    }

    /// Record a failure and trip the appropriate breakers based on severity.
    pub async fn record_failure(&self, failure: &OperationFailure) {
        match failure.severity {
            FailureSeverity::SymbolSpecific => {
                // Trip only the per-symbol breaker
                let sym = failure.symbol.clone();
                let breaker = self.symbol_breaker(&sym).await;
                breaker.manual_halt(&failure.reason).await;
            }
            FailureSeverity::ExchangeSpecific => {
                // Trip per-symbol AND per-exchange breakers
                let sym = failure.symbol.clone();
                let ex = failure.exchange.clone();
                {
                    let sym_breaker = self.symbol_breaker(&sym).await;
                    sym_breaker.manual_halt(&failure.reason).await;
                }
                {
                    let ex_breaker = self.exchange_breaker(&ex).await;
                    ex_breaker.manual_halt(&failure.reason).await;
                }
            }
            FailureSeverity::Systemic => {
                // Trip ALL breakers — system-wide halt
                let reason = failure.reason.clone();
                {
                    let breakers = self.symbol_breakers.read().await;
                    for breaker in breakers.values() {
                        breaker.manual_halt(&reason).await;
                    }
                }
                {
                    let breakers = self.exchange_breakers.read().await;
                    for breaker in breakers.values() {
                        breaker.manual_halt(&reason).await;
                    }
                }
                self.global_breaker.manual_halt(&reason).await;
            }
        }
    }

    /// Reset all breakers (manual resume).
    pub async fn reset_all(&self) {
        {
            let breakers = self.symbol_breakers.read().await;
            for breaker in breakers.values() {
                breaker.manual_reset().await;
            }
        }
        {
            let breakers = self.exchange_breakers.read().await;
            for breaker in breakers.values() {
                breaker.manual_reset().await;
            }
        }
        self.global_breaker.manual_reset().await;
    }

    /// Get current status of all three tiers.
    pub async fn status(&self) -> BreakerHierarchyStatus {
        let global = self.global_breaker.current_state().await;

        let symbol_states = {
            let breakers = self.symbol_breakers.read().await;
            let mut states = Vec::new();
            for (k, v) in breakers.iter() {
                states.push((k.clone(), v.current_state().await));
            }
            states
        };

        let exchange_states = {
            let breakers = self.exchange_breakers.read().await;
            let mut states = Vec::new();
            for (k, v) in breakers.iter() {
                states.push((k.clone(), v.current_state().await));
            }
            states
        };

        BreakerHierarchyStatus {
            global_state: global,
            symbol_breakers: symbol_states,
            exchange_breakers: exchange_states,
        }
    }
}

impl Default for CircuitBreakerHierarchy {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of the full breaker hierarchy status.
#[derive(Debug, Clone)]
pub struct BreakerHierarchyStatus {
    pub global_state: BreakerState,
    pub symbol_breakers: Vec<(String, BreakerState)>,
    pub exchange_breakers: Vec<(String, BreakerState)>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Global singleton references for easy access
// ═══════════════════════════════════════════════════════════════════════════════

/// Global bulkhead registry (lazily initialized).
pub static GLOBAL_BULKHEADS: Lazy<BulkheadRegistry> = Lazy::new(BulkheadRegistry::new);

/// Convenience: acquire a broker API permit.
pub async fn acquire_broker_permit() -> Result<BulkheadPermit, BulkheadFull> {
    GLOBAL_BULKHEADS.acquire(bulkhead_names::BROKER_API).await
}

/// Convenience: acquire a data feed permit.
pub async fn acquire_data_feed_permit() -> Result<BulkheadPermit, BulkheadFull> {
    GLOBAL_BULKHEADS.acquire(bulkhead_names::DATA_FEED).await
}

/// Convenience: acquire a database write permit.
pub async fn acquire_db_write_permit() -> Result<BulkheadPermit, BulkheadFull> {
    GLOBAL_BULKHEADS.acquire(bulkhead_names::DATABASE_WRITE).await
}

/// Convenience: acquire a database read permit.
pub async fn acquire_db_read_permit() -> Result<BulkheadPermit, BulkheadFull> {
    GLOBAL_BULKHEADS.acquire(bulkhead_names::DATABASE_READ).await
}

/// Convenience: acquire an agent evaluation permit.
pub async fn acquire_agent_permit() -> Result<BulkheadPermit, BulkheadFull> {
    GLOBAL_BULKHEADS.acquire(bulkhead_names::AGENT_EVALUATION).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bulkhead_acquire_and_release() {
        let pool = Bulkhead::new("test", 2, 100);
        let p1 = pool.acquire().await.unwrap();
        let p2 = pool.acquire().await.unwrap();
        // Third attempt should fail (timeout)
        let p3 = pool.acquire().await;
        assert!(p3.is_err());
        drop(p1);
        // Now one permit is available
        let p4 = pool.acquire().await;
        assert!(p4.is_ok());
        drop(p2);
        drop(p4);
    }

    #[tokio::test]
    async fn test_registry_all_pools_accessible() {
        let registry = BulkheadRegistry::new();
        for name in &[
            bulkhead_names::BROKER_API,
            bulkhead_names::DATA_FEED,
            bulkhead_names::DATABASE_WRITE,
            bulkhead_names::DATABASE_READ,
            bulkhead_names::AGENT_EVALUATION,
        ] {
            let permit = registry.acquire(name).await;
            assert!(permit.is_ok(), "Pool '{}' should be acquirable", name);
        }
    }

    #[tokio::test]
    async fn test_hierarchy_allows_trading_initially() {
        let hierarchy = CircuitBreakerHierarchy::new();
        assert!(hierarchy.is_trading_allowed("BTC", "binance").await);
    }

    #[tokio::test]
    async fn test_hierarchy_symbol_halt_blocks_symbol_only() {
        let hierarchy = CircuitBreakerHierarchy::new();
        let failure = OperationFailure {
            symbol: "BTC".to_string(),
            exchange: "binance".to_string(),
            severity: FailureSeverity::SymbolSpecific,
            reason: "Illiquid market".to_string(),
        };
        hierarchy.record_failure(&failure).await;
        // BTC trading on binance should be blocked
        assert!(!hierarchy.is_trading_allowed("BTC", "binance").await);
        // ETH trading on binance should still be allowed
        assert!(hierarchy.is_trading_allowed("ETH", "binance").await);
    }

    #[tokio::test]
    async fn test_hierarchy_global_halt_blocks_all() {
        let hierarchy = CircuitBreakerHierarchy::new();
        let failure = OperationFailure {
            symbol: "BTC".to_string(),
            exchange: "binance".to_string(),
            severity: FailureSeverity::Systemic,
            reason: "Memory pressure".to_string(),
        };
        hierarchy.record_failure(&failure).await;
        assert!(!hierarchy.is_trading_allowed("BTC", "binance").await);
        assert!(!hierarchy.is_trading_allowed("ETH", "binance").await);
        assert!(!hierarchy.is_trading_allowed("BTC", "coinbase").await);
    }

    #[tokio::test]
    async fn test_hierarchy_reset() {
        let hierarchy = CircuitBreakerHierarchy::new();
        let f2 = OperationFailure {
            symbol: "SOL".to_string(),
            exchange: "binance".to_string(),
            severity: FailureSeverity::Systemic,
            reason: "Test".to_string(),
        };
        hierarchy.record_failure(&f2).await;
        assert!(!hierarchy.is_trading_allowed("SOL", "binance").await);
        hierarchy.reset_all().await;
        assert!(hierarchy.is_trading_allowed("SOL", "binance").await);
    }
}
