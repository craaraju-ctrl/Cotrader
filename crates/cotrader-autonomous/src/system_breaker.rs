//! # SystemCircuitBreaker — Coordinated Circuit Breaker Across Subsystems
//!
//! Provides system-wide halt coordination when any subsystem detects a critical issue.
//! When one circuit breaker halts, it notifies all others to prevent cascading failures.

use crate::circuit_breaker::{BreakerState, CircuitBreaker};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Source of a system halt
#[derive(Debug, Clone, PartialEq)]
pub enum BreakerSource {
    Trading,
    Memory,
    Risk,
    Manual,
}

impl std::fmt::Display for BreakerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BreakerSource::Trading => write!(f, "TRADING"),
            BreakerSource::Memory => write!(f, "MEMORY"),
            BreakerSource::Risk => write!(f, "RISK"),
            BreakerSource::Manual => write!(f, "MANUAL"),
        }
    }
}

/// Event emitted when a subsystem halts
#[derive(Debug, Clone)]
pub struct BreakerEvent {
    pub source: BreakerSource,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// System-wide coordinated circuit breaker
///
/// Monitors trading, memory, and risk subsystems. When any subsystem halts,
/// it broadcasts the halt to all other subsystems.
pub struct SystemCircuitBreaker {
    /// Trading circuit breaker (from rat-autonomous)
    trading: Arc<CircuitBreaker>,
    /// Memory circuit breaker (from memory crate)
    memory_failing: Arc<tokio::sync::RwLock<bool>>,
    /// Risk hard stop (from rat-runtime)
    risk_halted: Arc<tokio::sync::RwLock<bool>>,
    /// Broadcast channel for halt events
    event_tx: broadcast::Sender<BreakerEvent>,
    /// Track if system is globally halted
    global_halt: Arc<tokio::sync::RwLock<bool>>,
}

impl SystemCircuitBreaker {
    /// Create a new coordinated circuit breaker
    pub fn new(trading: Arc<CircuitBreaker>) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            trading,
            memory_failing: Arc::new(tokio::sync::RwLock::new(false)),
            risk_halted: Arc::new(tokio::sync::RwLock::new(false)),
            event_tx,
            global_halt: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }

    /// Subscribe to breaker events
    pub fn subscribe(&self) -> broadcast::Receiver<BreakerEvent> {
        self.event_tx.subscribe()
    }

    /// Check if trading is allowed (all subsystems healthy)
    pub async fn is_trading_allowed(&self) -> bool {
        // Check global halt first
        if *self.global_halt.read().await {
            return false;
        }

        // Check trading circuit breaker
        if !self.trading.is_trading_allowed().await {
            return false;
        }

        // Check memory subsystem
        if *self.memory_failing.read().await {
            return false;
        }

        // Check risk subsystem
        if *self.risk_halted.read().await {
            return false;
        }

        true
    }

    /// Report a memory subsystem failure
    pub async fn report_memory_failure(&self, reason: &str) {
        *self.memory_failing.write().await = true;
        self.broadcast_halt(BreakerSource::Memory, reason).await;
    }

    /// Report memory subsystem recovery
    pub async fn report_memory_recovery(&self) {
        *self.memory_failing.write().await = false;
        self.check_auto_resume().await;
    }

    /// Report a risk subsystem halt
    pub async fn report_risk_halt(&self, reason: &str) {
        *self.risk_halted.write().await = true;
        self.broadcast_halt(BreakerSource::Risk, reason).await;
    }

    /// Report risk subsystem recovery
    pub async fn report_risk_recovery(&self) {
        *self.risk_halted.write().await = false;
        self.check_auto_resume().await;
    }

    /// Manually trigger a global halt
    pub async fn halt(&self, reason: &str) {
        *self.global_halt.write().await = true;
        self.broadcast_halt(BreakerSource::Manual, reason).await;
    }

    /// Manually resume from global halt
    pub async fn resume(&self) {
        *self.global_halt.write().await = false;
        *self.memory_failing.write().await = false;
        *self.risk_halted.write().await = false;
        
        let _ = self.event_tx.send(BreakerEvent {
            source: BreakerSource::Manual,
            reason: "System resumed".to_string(),
            timestamp: chrono::Utc::now(),
        });
    }

    /// Get current system status
    pub async fn status(&self) -> SystemStatus {
        SystemStatus {
            trading_state: self.trading.current_state().await,
            memory_failing: *self.memory_failing.read().await,
            risk_halted: *self.risk_halted.read().await,
            global_halt: *self.global_halt.read().await,
        }
    }

    /// Broadcast a halt event to all subscribers
    async fn broadcast_halt(&self, source: BreakerSource, reason: &str) {
        let event = BreakerEvent {
            source,
            reason: reason.to_string(),
            timestamp: chrono::Utc::now(),
        };
        
        println!(
            "[SystemBreaker] 🛑 HALT from {}: {}",
            event.source, event.reason
        );
        
        let _ = self.event_tx.send(event);
    }

    /// Check if all subsystems have recovered and auto-resume
    async fn check_auto_resume(&self) {
        if *self.global_halt.read().await {
            return;
        }

        let memory_ok = !*self.memory_failing.read().await;
        let risk_ok = !*self.risk_halted.read().await;
        let trading_ok = self.trading.current_state().await == BreakerState::Armed;

        if memory_ok && risk_ok && trading_ok {
            println!("[SystemBreaker] ✅ All subsystems recovered — system operational");
        }
    }
}

/// System status snapshot
#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub trading_state: BreakerState,
    pub memory_failing: bool,
    pub risk_halted: bool,
    pub global_halt: bool,
}

impl std::fmt::Display for SystemStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.global_halt {
            write!(f, "GLOBAL_HALT")
        } else if self.memory_failing {
            write!(f, "MEMORY_DEGRADED")
        } else if self.risk_halted {
            write!(f, "RISK_HALT")
        } else {
            match self.trading_state {
                BreakerState::Armed => write!(f, "OPERATIONAL"),
                BreakerState::Recovery => write!(f, "RECOVERING"),
                BreakerState::Halted => write!(f, "TRADING_HALT"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::CircuitBreakerConfig;

    #[tokio::test]
    async fn test_system_breaker_all_healthy() {
        let trading = Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default()));
        let system = SystemCircuitBreaker::new(trading);

        assert!(system.is_trading_allowed().await);
    }

    #[tokio::test]
    async fn test_system_breaker_memory_failure() {
        let trading = Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default()));
        let system = SystemCircuitBreaker::new(trading);

        system.report_memory_failure("Connection lost").await;
        assert!(!system.is_trading_allowed().await);

        system.report_memory_recovery().await;
        assert!(system.is_trading_allowed().await);
    }

    #[tokio::test]
    async fn test_system_breaker_global_halt() {
        let trading = Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default()));
        let system = SystemCircuitBreaker::new(trading);

        system.halt("Manual override").await;
        assert!(!system.is_trading_allowed().await);

        system.resume().await;
        assert!(system.is_trading_allowed().await);
    }

    #[tokio::test]
    async fn test_system_breaker_event_broadcast() {
        let trading = Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default()));
        let system = SystemCircuitBreaker::new(trading);

        let mut rx = system.subscribe();

        system.report_memory_failure("Test failure").await;

        let event = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(event.source, BreakerSource::Memory);
        assert_eq!(event.reason, "Test failure");
    }
}
