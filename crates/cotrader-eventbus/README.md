# cotrader-eventbus

A **subject-based pub-sub event bus** for the rat trading system. Wraps `tokio::sync::broadcast` with NATS-style subject filtering and strongly-typed event variants.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Subjects](#subjects)
  - [Pattern Matching](#pattern-matching)
  - [Pre-built Subjects](#pre-built-subjects)
- [Events](#events)
  - [RatEvent Enum](#ratevent-enum)
  - [Event Payloads](#event-payloads)
- [EventBus API](#eventbus-api)
  - [Creating a Bus](#creating-a-bus)
  - [Publishing Events](#publishing-events)
  - [Subscribing](#subscribing)
  - [EventStream](#eventstream)
- [Real-world Usage](#real-world-usage)
  - [Orchestrator Integration](#orchestrator-integration)
  - [Multiple Subscribers](#multiple-subscribers)
  - [WebSocket Bridge](#websocket-bridge)
- [Best Practices](#best-practices)

---

## Overview

`cotrader-eventbus` provides a lightweight, in-process event bus for the rat trading system. It uses **subjects** (dot-separated topic strings with wildcards) to route events, so publishers and subscribers are fully decoupled.

**Key design decisions:**

- **Subject-based routing** — Like NATS: subscribe with `signal.>`, publish to `signal.BTC`. No channel enumeration needed.
- **Broadcast semantics** — Every subscriber gets every event (subject-filtered). Built on `tokio::sync::broadcast`.
- **Strongly typed** — Events are a Rust enum, not raw JSON. Match on variant, access typed fields.
- **Clone-able handles** — `EventBus` is cheaply cloneable. Pass clones across threads, tasks, and components.

---

## Quick Start

```rust
use cotrader_eventbus::{EventBus, Subject, RatEvent, SignalEvent};

// 1. Create a bus (capacity = max buffered events per subscriber)
let bus = EventBus::new(256);

// 2. Subscribe with a pattern
let mut rx = bus.subscribe(Subject::new("signal.>"));

// 3. Publish an event
let event = RatEvent::Signal(SignalEvent {
    symbol: "BTC".into(),
    action: "BUY".into(),
    entry_price: 50000.0,
    stop_loss: 49000.0,
    take_profit: 52000.0,
    confidence: 0.85,
    reasoning: "Strong support bounce".into(),
    source: "strategy_decision".into(),
    timestamp_micros: chrono::Utc::now().timestamp_micros(),
});

bus.publish(Subject::new("signal.BTC"), event);

// 4. Receive (blocking-free with subject filtering)
while let Some((subject, event)) = rx.recv().await {
    println!("Received {}: {}", subject, event.summary());
}
```

---

## Architecture

```
┌──────────────┐     publish("signal.BTC", SignalEvent{...})
│  Publisher   │─────────────────────────▶┐
│  (loop.rs)   │                          │
└──────────────┘                          ▼
                                   ┌──────────────┐
                                   │  EventBus    │
                                   │  (broadcast  │
                                   │   channel)   │
                                   └──────┬───────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
                    ▼                     ▼                     ▼
           ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
           │ EventStream  │     │ EventStream  │     │ EventStream  │
           │ sub: >       │     │ sub: signal.>│     │ sub: price.* │
           │ (catch-all)  │     │ (signals)    │     │ (prices)     │
           └──────────────┘     └──────────────┘     └──────────────┘
                  │                     │                     │
                  ▼                     ▼                     ▼
           Subscriber A          Subscriber B          Subscriber C
           (all events)          (signals only)        (prices only)
```

Each subscriber gets its own receive buffer from the shared broadcast channel. The `EventStream` filters out non-matching events transparently.

---

## Subjects

A **subject** is a dot-separated string used to route events. Subjects are case-sensitive by convention (use uppercase for symbols: `signal.BTC`).

```rust
use cotrader_eventbus::Subject;

// Exact subject
let s1 = Subject::new("signal.BTC");

// Pattern with wildcard
let s2 = Subject::new("market.price.>");

// From &str or String
let s3: Subject = "health.orchestrator".into();
let s4: Subject = String::from("system.control").into();
```

### Pattern Matching

| Pattern | Matches | Does Not Match |
|---|---|---|
| `signal.BTC` | `signal.BTC` | `signal.ETH`, `signal.BTC.detail` |
| `signal.*` | `signal.BTC`, `signal.ETH` | `signal.BTC.detail`, `other.BTC` |
| `signal.>` | `signal.BTC`, `signal.BTC.detail` | `market.BTC` |
| `>` | everything | — |
| `signal.BTC.detail` | `signal.BTC.detail` | `signal.BTC` |

Rules:
- `*` matches exactly one dot-separated token
- `>` matches one or more trailing tokens — must be the **last** token
- `>` alone is a catch-all
- An exact match is always preferred

### Pre-built Subjects

The `subjects` module provides factory functions for commonly used subjects:

```rust
use cotrader_eventbus::subjects;

// Signal events
subjects::all_signal_events();   // "signal.>"
subjects::signal("BTC");         // "signal.BTC"

// Market data
subjects::market_price("BTC");   // "market.price.BTC"

// Portfolio
subjects::portfolio_snapshot();  // "portfolio.snapshot"

// Health checks
subjects::health("orchestrator"); // "health.orchestrator"

// System control
subjects::system_control();       // "system.control"

// Catch-all
subjects::all();                  // ">"
```

---

## Events

### RatEvent Enum

The `RatEvent` enum is the core event type. Every event published on the bus is one of these variants:

```rust
pub enum RatEvent {
    Signal(SignalEvent),                 // Trade signal (BUY/SELL/HOLD)
    MarketPrice(MarketPriceEvent),       // Live price tick
    PortfolioSnapshot(PortfolioSnapshotEvent),  // Periodic portfolio summary
    Health(HealthEvent),                 // Service health check
    SystemControl(SystemControlEvent),   // System commands
}
```

Each variant is serialized with `#[serde(tag = "type")]`, making it easy to round-trip through JSON:

```json
{"type": "Signal", "symbol": "BTC", "action": "BUY", ...}
```

### Event Payloads

**`SignalEvent`** — Emitted when a trade signal passes all guards:

| Field | Type | Description |
|---|---|---|
| `symbol` | `String` | Trading pair or symbol |
| `action` | `String` | `"BUY"`, `"SELL"`, or `"HOLD"` |
| `entry_price` | `f64` | Proposed entry price |
| `stop_loss` | `f64` | Stop loss price |
| `take_profit` | `f64` | Take profit price |
| `confidence` | `f64` | Signal confidence (0.0–1.0) |
| `reasoning` | `String` | Human-readable rationale |
| `source` | `String` | Source agent or module |
| `timestamp_micros` | `i64` | Unix timestamp in microseconds |

**`MarketPriceEvent`** — A live price tick:

| Field | Type | Description |
|---|---|---|
| `symbol` | `String` | Trading pair |
| `price` | `f64` | Current price |
| `exchange` | `String` | Source exchange (e.g. `"binance"`) |
| `timestamp_micros` | `i64` | Timestamp |

**`PortfolioSnapshotEvent`** — Periodic portfolio summary:

| Field | Type | Description |
|---|---|---|
| `total_equity` | `f64` | Current equity |
| `cash_balance` | `f64` | Available cash |
| `daily_pnl` | `f64` | Realized P&L today |
| `timestamp_micros` | `i64` | Timestamp |

**`HealthEvent`** — Service health status:

| Field | Type | Description |
|---|---|---|
| `service` | `String` | Service name |
| `healthy` | `bool` | Whether the service is healthy |
| `latency_ms` | `Option<u64>` | Response latency, if available |
| `timestamp_micros` | `i64` | Timestamp |

**`SystemControlEvent`** — System-level commands:

| Field | Type | Description |
|---|---|---|
| `command` | `String` | `"START"`, `"STOP"`, or `"RESTART"` |
| `target` | `Option<String>` | Optional target component |
| `reason` | `String` | Reason for the command |
| `timestamp_micros` | `i64` | Timestamp |

### Helper Methods

```rust
// Get the conventional subject for this event
let subject = event.default_subject();  // e.g. "signal.BTC"

// Get a one-line summary for logging
let summary = event.summary();  // e.g. "Signal BTC BUY @ 50000.00 (conf=0.85)"
```

---

## EventBus API

### Creating a Bus

```rust
// Capacity = max buffered events per subscriber.
// Events beyond capacity are dropped for the slowest subscriber.
let bus = EventBus::new(512);
```

`EventBus` is `Clone` — share it across tasks and components:

```rust
let bus = EventBus::new(256);
let bus_clone = bus.clone();  // cheap — Arc<AtomicU64> + Sender clone
```

### Publishing Events

```rust
// Using Subject directly:
bus.publish(Subject::new("signal.BTC"), RatEvent::Signal(signal));

// Using the subjects helper module:
bus.publish(subjects::market_price("BTC"), RatEvent::MarketPrice(price));

// Using a string (converts via `Into<Subject>`):
bus.publish("health.orchestrator", RatEvent::Health(health));
```

`publish()` is non-blocking — it returns immediately after sending to the broadcast channel. If a subscriber's buffer is full, the oldest event is dropped for that subscriber.

Every 100th publish is logged via `tracing::info!(target: "event_bus", ...)` for observability.

### Subscribing

```rust
// Subscribe with a Subject pattern:
let mut rx = bus.subscribe(Subject::new("signal.>"));

// Using the subjects helper:
let mut rx = bus.subscribe(subjects::all_signal_events());

// Using a string:
let mut rx = bus.subscribe("signal.>");
```

### EventStream

`EventStream` is returned by `subscribe()` and provides two receive methods:

```rust
// Async receive — waits for the next matching event
// Returns None when all senders are dropped
while let Some((subject, event)) = rx.recv().await {
    match event {
        RatEvent::Signal(s) => handle_signal(s),
        RatEvent::MarketPrice(p) => handle_price(p),
        _ => {}  // skip other event types
    }
}

// Try receive — returns None if no matching event is available
if let Some((subject, event)) = rx.try_recv() {
    println!("Immediate event: {}", event.summary());
}
```

**Behavior:**
- Non-matching events are silently skipped (the stream only yields events matching the subscription pattern)
- Lagged events (subscriber too slow) are skipped with a `warn!` log
- Channel closure (all publishers dropped) returns `None`
- Try-recv returns `None` on empty or closed channels

---

## Real-world Usage

### Orchestrator Integration

The orchestrator creates a single `EventBus` at startup, publishes a `SystemControl::START` event, and shares clones across three background loops:

```rust
// In main.rs — startup
let event_bus = EventBus::new(512);

bus.publish(Subject::new("system.control"), RatEvent::SystemControl(
    SystemControlEvent {
        command: "START".into(),
        target: None,
        reason: "Orchestrator startup".into(),
        timestamp_micros: Utc::now().timestamp_micros(),
    }
));

// Pass clone to each loop
let fast_handle = tokio::spawn(fast_loop(..., event_bus.clone()));
let medium_handle = tokio::spawn(medium_loop(..., event_bus.clone()));
```

**Fast loop** publishes price ticks, portfolio snapshots, and health events:

```rust
// Price tick (every 5s per symbol)
event_bus.publish(
    subjects::market_price(&sym),
    RatEvent::MarketPrice(MarketPriceEvent {
        symbol: sym,
        price,
        exchange: "binance".into(),
        timestamp_micros: Utc::now().timestamp_micros(),
    }),
);

// Portfolio snapshot (every ~60s)
event_bus.publish(
    subjects::portfolio_snapshot(),
    RatEvent::PortfolioSnapshot(PortfolioSnapshotEvent { ... }),
);

// Health check (every ~60s)
event_bus.publish(
    subjects::health("orchestrator"),
    RatEvent::Health(HealthEvent { ... }),
);
```

**Medium loop** publishes signal events on trade execution:

```rust
event_bus.publish(
    subjects::signal(&sym),
    RatEvent::Signal(SignalEvent {
        symbol: signal.symbol,
        action: "BUY".into(),
        entry_price: signal.entry_price,
        stop_loss: signal.stop_loss,
        take_profit: signal.take_profit,
        confidence: signal.confidence_score,
        reasoning: signal.reasoning,
        source: "pipeline".into(),
        timestamp_micros: Utc::now().timestamp_micros(),
    }),
);
```

### Multiple Subscribers

```rust
// Two independent subscribers on the same bus
let bus = EventBus::new(256);

// Subscriber A: everything
let mut rx_all = bus.subscribe(subjects::all());

// Subscriber B: only signals
let mut rx_signals = bus.subscribe(subjects::all_signal_events());

// Both receive matching events independently
bus.publish(
    subjects::signal("BTC"),
    RatEvent::Signal(SignalEvent { symbol: "BTC".into(), ... }),
);
// → rx_all receives it
// → rx_signals receives it

bus.publish(
    subjects::market_price("BTC"),
    RatEvent::MarketPrice(MarketPriceEvent { symbol: "BTC".into(), ... }),
);
// → rx_all receives it
// → rx_signals does NOT receive it (non-matching subject pattern)
```

### WebSocket Bridge

The WebSocket bridge subscribes to all signal events and forwards them as JSON:

```rust
let mut signal_rx = event_bus.subscribe(subjects::all_signal_events());
let update_tx = orchestrator.state.io.update_tx.clone();

tokio::spawn(async move {
    loop {
        match signal_rx.recv().await {
            Some((_subject, event)) => {
                let msg = serde_json::json!({
                    "type": "signal_event",
                    "event": &event,
                })
                .to_string();
                let _ = update_tx.send(msg);
            }
            None => break,
        }
    }
});
```

---

## Best Practices

1. **Choose capacity wisely** — Set capacity to the maximum number of events a subscriber might miss while busy. `512` is a good default for the orchestrator. Monitor lag events in logs.

2. **Clone before spawn** — When using `event_bus` inside a `tokio::spawn` inside a loop, always clone first:

    ```rust
    // ✅ Correct:
    for item in items {
        let eb = event_bus.clone();
        tokio::spawn(async move {
            eb.publish(subject, event);
        });
    }

    // ❌ Wrong — event_bus moved in first iteration:
    for item in items {
        tokio::spawn(async move {
            event_bus.publish(subject, event);  // compile error
        });
    }
    ```

3. **Use `subjects` helpers** — They're guaranteed to produce the correct subject strings. Avoid hardcoding `"signal.BTC"` — use `subjects::signal("BTC")` instead.

4. **Match on variant, not subject** — `RatEvent` is an enum. Use pattern matching on the variant rather than parsing the subject string to determine event type:

    ```rust
    // ✅ Prefer this:
    match event {
        RatEvent::Signal(s) => handle_signal(s),
        _ => {}
    }

    // ❌ Avoid this:
    if subject.as_str().starts_with("signal.") {
        // manually parse...
    }
    ```

5. **Non-blocking publish** — `publish()` never blocks. If the broadcast channel is full, the slowest subscriber loses events silently (with a warning log). This prevents a slow consumer from blocking producers.

6. **`try_recv` for polling** — Use `try_recv()` in tight polling loops where blocking would be inappropriate. Use `recv().await` in event-driven loops where you want to sleep until the next event.
