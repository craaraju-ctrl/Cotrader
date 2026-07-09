//! cotrader-eventbus — Subject-based pub-sub event bus for the rat trading system.
//!
//! Provides an `EventBus` that wraps `tokio::sync::broadcast` with NATS-style
//! subject filtering, plus a strongly-typed `RatEvent` enum matching the
//! events the orchestrator and pipeline produce.
//!
//! # Quick Start
//! ```ignore
//! use cotrader_eventbus::{EventBus, Subject, RatEvent, SignalEvent};
//!
//! let bus = EventBus::new(256);
//! let mut rx = bus.subscribe(Subject::new("signal.>"));
//!
//! bus.publish(
//!     Subject::new("signal.BTC"),
//!     RatEvent::Signal(SignalEvent { /* ... */ }),
//! );
//!
//! while let Some((subject, event)) = rx.recv().await {
//!     println!("{}: {}", subject, event.summary());
//! }
//! ```

pub mod bus;
pub mod event;
pub mod subject;

pub use bus::{subjects, EventBus, EventStream};
pub use event::{
    HealthEvent, MarketPriceEvent, PortfolioSnapshotEvent, RatEvent, SignalEvent,
    SystemControlEvent,
};
pub use subject::Subject;
