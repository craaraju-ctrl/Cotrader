//! # BreakerNet — global circuit-breaker coordination channel
//!
//! Fault-isolation audit finding D4: the three breakers (rat-autonomous
//! trading `CircuitBreaker`, agentic-memory `ResilientClient` breaker,
//! rat-runtime `RiskManager` hard stop) were fully decoupled — a halt in any
//! one was invisible to the others (confirmed live: RiskManager kept
//! APPROVING trades while the trading breaker was HALTED).
//!
//! This module is the shared halt spine:
//! - `announce_halt(origin, reason)` — trips the halt flag for `origin` and
//!   broadcasts a `BreakerEvent` to all subscribers.
//! - `is_halted()` / `is_halted_by_other(me)` — one atomic scan; safe on every
//!   hot path. Any subsystem must check before executing a trade/order.
//! - `resume(origin)` — explicit resume (manual reset, cool-down expiry).
//! - `subscribe()` — live `BreakerEvent` stream for TUI/telemetry/watchdog.
//!
//! Works without a Tokio runtime (flags are plain atomics; the broadcast
//! channel only needs a runtime on the *receiving* side).

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerId {
    Trading,
    Memory,
    Risk,
}

impl std::fmt::Display for BreakerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BreakerId::Trading => write!(f, "trading-breaker"),
            BreakerId::Memory => write!(f, "memory-breaker"),
            BreakerId::Risk => write!(f, "risk-manager"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BreakerEvent {
    Halt { origin: BreakerId, reason: String },
    Resume { origin: BreakerId },
}

struct Net {
    tx: broadcast::Sender<BreakerEvent>,
    halted: [AtomicBool; 3], // one flag per BreakerId
}

fn idx(id: BreakerId) -> usize {
    match id {
        BreakerId::Trading => 0,
        BreakerId::Memory => 1,
        BreakerId::Risk => 2,
    }
}

static NET: Lazy<Net> = Lazy::new(|| {
    let (tx, _) = broadcast::channel(64);
    Net {
        tx,
        halted: [
            AtomicBool::new(false),
            AtomicBool::new(false),
            AtomicBool::new(false),
        ],
    }
});

/// Trip the global halt latch for `origin`. Synchronous, idempotent.
/// Every other layer that consults `is_halted_by_other()` stops immediately.
pub fn announce_halt(origin: BreakerId, reason: &str) {
    let was = NET.halted[idx(origin)].swap(true, Ordering::SeqCst);
    if !was {
        log::error!("[BreakerNet] GLOBAL HALT from {}: {}", origin, reason);
        eprintln!("[BreakerNet] 🚨 GLOBAL HALT from {}: {}", origin, reason);
    }
    let _ = NET.tx.send(BreakerEvent::Halt {
        origin,
        reason: reason.to_string(),
    });
}

/// Resume for `origin` (manual reset, cool-down expiry, breaker close).
pub fn resume(origin: BreakerId) {
    NET.halted[idx(origin)].store(false, Ordering::SeqCst);
    log::warn!("[BreakerNet] resume announced by {}", origin);
    let _ = NET.tx.send(BreakerEvent::Resume { origin });
}

/// True if ANY subsystem is halted — gate for execution hot paths.
#[inline]
pub fn is_halted() -> bool {
    NET.halted.iter().any(|f| f.load(Ordering::SeqCst))
}

/// True if any subsystem OTHER than `me` is halted. Components use this so
/// their own local halt state (already handled locally) is not double-counted.
#[inline]
pub fn is_halted_by_other(me: BreakerId) -> bool {
    NET.halted
        .iter()
        .enumerate()
        .any(|(i, f)| i != idx(me) && f.load(Ordering::SeqCst))
}

/// Live event stream for observers (TUI, watchdog, journal).
pub fn subscribe() -> broadcast::Receiver<BreakerEvent> {
    NET.tx.subscribe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn halt_is_global_and_observable() {
        let mut rx = subscribe();
        announce_halt(BreakerId::Trading, "5% slippage");
        assert!(is_halted(), "any origin must trip the global latch");
        assert!(
            is_halted_by_other(BreakerId::Memory),
            "memory layer must see it"
        );
        assert!(
            is_halted_by_other(BreakerId::Risk),
            "risk layer must see it"
        );
        assert!(
            !is_halted_by_other(BreakerId::Trading),
            "own halt not double-counted"
        );
        match rx.recv().await.unwrap() {
            BreakerEvent::Halt { origin, .. } => assert_eq!(origin, BreakerId::Trading),
            _ => panic!("expected Halt event"),
        }
        resume(BreakerId::Trading);
        assert!(!is_halted());
    }
}
