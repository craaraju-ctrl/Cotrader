use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::event::RatEvent;
use crate::subject::Subject;

/// A subject-based pub-sup event bus for the rat trading system.
///
/// Wraps `tokio::sync::broadcast` and adds subject-level filtering so
/// subscribers only receive events published on matching subjects.
///
/// # Example
/// ```ignore
/// let bus = EventBus::new(256);
///
/// // Subscriber A: all signal events
/// let mut sig_rx = bus.subscribe(Subject::new("signal.>"));
///
/// // Publisher: a BUY signal for BTC
/// bus.publish(Subject::new("signal.BTC"), RatEvent::Signal(...));
///
/// // Subscriber A receives it (pattern signal.> matches signal.BTC)
/// let (subject, event) = sig_rx.recv().await.unwrap();
/// ```
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<(Subject, RatEvent)>,
    published_count: Arc<AtomicU64>,
}

impl EventBus {
    /// Create a new EventBus with the given channel capacity.
    /// Events beyond this capacity are dropped for the slowest subscriber.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            published_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Publish an event under the given subject.
    ///
    /// All subscribers whose subscription pattern matches `subject` will
    /// receive the event (unless their receive buffer is full).
    pub fn publish<S>(&self, subject: S, event: RatEvent)
    where
        S: Into<Subject>,
    {
        let subject = subject.into();
        let count = self.published_count.fetch_add(1, Ordering::SeqCst);
        if count % 100 == 0 {
            tracing::info!(target: "event_bus", published = count, subject = %subject, event = %event.summary(), "Published event");
        }
        let _ = self.sender.send((subject, event));
    }

    /// Subscribe to events matching the given subject pattern.
    ///
    /// Returns an `EventStream` that yields `(Subject, RatEvent)` pairs
    /// for every published event whose subject matches this pattern.
    pub fn subscribe<S>(&self, pattern: S) -> EventStream
    where
        S: Into<Subject>,
    {
        let rx = self.sender.subscribe();
        EventStream {
            rx,
            pattern: pattern.into(),
        }
    }

    /// Get a sender handle for components that need to publish events.
    pub fn sender(&self) -> broadcast::Sender<(Subject, RatEvent)> {
        self.sender.clone()
    }

    /// Total number of events published since creation.
    pub fn published_count(&self) -> u64 {
        self.published_count.load(Ordering::SeqCst)
    }

    /// Number of active subscribers (receivers) on this event bus.
    ///
    /// Each call to `subscribe()` creates a new receiver, and when the
    /// returned `EventStream` is dropped the receiver count decreases.
    /// Use this to detect subscriber leaks or confirm fan-out.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Create a new EventBus with a pre-existing sender.
    /// Used internally when cloning the bus from a sender handle.
    pub fn with_sender(sender: broadcast::Sender<(Subject, RatEvent)>) -> Self {
        Self {
            sender,
            published_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// A stream of events filtered by a subject pattern.
///
/// This is the receiver side of a subscription. `recv()` skips any events
/// whose subject does not match the subscription pattern.
#[derive(Debug)]
pub struct EventStream {
    rx: broadcast::Receiver<(Subject, RatEvent)>,
    pattern: Subject,
}

impl EventStream {
    /// Receive the next matching event.
    ///
    /// Returns `None` if the channel has been closed (all senders dropped).
    /// Events that don't match the subscription pattern are silently skipped.
    /// If the subscriber is too slow, lagged events are also skipped.
    pub async fn recv(&mut self) -> Option<(Subject, RatEvent)> {
        loop {
            match self.rx.recv().await {
                Ok((subject, event)) => {
                    if self.pattern.matches(&subject) {
                        return Some((subject, event));
                    }
                    // Skip non-matching events
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(target: "event_bus", lagged = n, "Event stream lagged, skipped {} events", n);
                    continue;
                }
            }
        }
    }

    /// Try to receive without blocking. Returns `None` if no matching event
    /// is immediately available.
    pub fn try_recv(&mut self) -> Option<(Subject, RatEvent)> {
        loop {
            match self.rx.try_recv() {
                Ok((subject, event)) => {
                    if self.pattern.matches(&subject) {
                        return Some((subject, event));
                    }
                    continue;
                }
                Err(broadcast::error::TryRecvError::Empty) => return None,
                Err(broadcast::error::TryRecvError::Closed) => return None,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!(target: "event_bus", lagged = n, "Event stream lagged, skipped {} events", n);
                    continue;
                }
            }
        }
    }
}

/// Subject helper functions matching the expected `event_subjects` API.
pub mod subjects {
    use crate::subject::Subject;

    /// Subject that matches ALL signal events for any symbol.
    pub fn all_signal_events() -> Subject {
        Subject::new("signal.>")
    }

    /// Subject for market price updates for a given symbol.
    pub fn market_price(symbol: &str) -> Subject {
        Subject::new(&format!("market.price.{}", symbol))
    }

    /// Subject for portfolio snapshot events.
    pub fn portfolio_snapshot() -> Subject {
        Subject::new("portfolio.snapshot")
    }

    /// Subject for health check events for a given service.
    pub fn health(service: &str) -> Subject {
        Subject::new(&format!("health.{}", service))
    }

    /// Subject for signal events for a given symbol.
    pub fn signal(symbol: &str) -> Subject {
        Subject::new(&format!("signal.{}", symbol))
    }

    /// Subject for system control events.
    pub fn system_control() -> Subject {
        Subject::new("system.control")
    }

    /// Catch-all subject that matches everything.
    pub fn all() -> Subject {
        Subject::new(">")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::*;
    use std::time::Duration;

    fn make_signal(symbol: &str, action: &str) -> RatEvent {
        RatEvent::Signal(SignalEvent {
            symbol: symbol.to_string(),
            action: action.to_string(),
            entry_price: 50000.0,
            stop_loss: 49000.0,
            take_profit: 52000.0,
            confidence: 0.85,
            reasoning: "test".to_string(),
            source: "unit_test".to_string(),
            timestamp_micros: chrono::Utc::now().timestamp_micros(),
        })
    }

    #[tokio::test]
    async fn test_publish_subscribe_exact_match() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe(Subject::new("signal.BTC"));

        bus.publish(Subject::new("signal.BTC"), make_signal("BTC", "BUY"));

        let (subject, event) = rx.recv().await.unwrap();
        assert_eq!(subject.as_str(), "signal.BTC");
        match event {
            RatEvent::Signal(s) => assert_eq!(s.symbol, "BTC"),
            _ => panic!("Expected Signal event"),
        }
    }

    #[tokio::test]
    async fn test_subject_pattern_wildcard() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe(Subject::new("signal.>"));

        bus.publish(Subject::new("signal.BTC"), make_signal("BTC", "BUY"));
        bus.publish(Subject::new("signal.ETH"), make_signal("ETH", "SELL"));

        let mut count = 0;
        for _ in 0..2 {
            if let Some((_subject, event)) = rx.recv().await {
                match event {
                    RatEvent::Signal(s) => count += 1,
                    _ => {}
                }
            }
        }
        assert_eq!(count, 2, "Should receive both matching events");
    }

    #[tokio::test]
    async fn test_non_matching_subject_skipped() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe(Subject::new("signal.BTC"));

        // Publish a non-matching event
        bus.publish(Subject::new("market.price.BTC"), RatEvent::MarketPrice(MarketPriceEvent {
            symbol: "BTC".to_string(),
            price: 50000.0,
            exchange: "binance".to_string(),
            timestamp_micros: chrono::Utc::now().timestamp_micros(),
        }));

        // The market price event should be skipped by the signal subscription

        // Now publish a matching event
        bus.publish(Subject::new("signal.BTC"), make_signal("BTC", "BUY"));

        // Should only receive the matching event
        let received = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("Should receive within timeout")
            .expect("Should be Some");
        assert_eq!(received.0.as_str(), "signal.BTC");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe(Subject::new("signal.>"));
        let mut rx2 = bus.subscribe(Subject::new("signal.>"));

        bus.publish(Subject::new("signal.BTC"), make_signal("BTC", "BUY"));

        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();
        assert_eq!(r1.0.as_str(), "signal.BTC");
        assert_eq!(r2.0.as_str(), "signal.BTC");
    }

    #[tokio::test]
    async fn test_catch_all_subject() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe(Subject::new(">"));

        bus.publish(Subject::new("signal.BTC"), make_signal("BTC", "BUY"));
        bus.publish(Subject::new("system.control"), RatEvent::SystemControl(SystemControlEvent {
            command: "START".to_string(),
            target: None,
            reason: "test".to_string(),
            timestamp_micros: chrono::Utc::now().timestamp_micros(),
        }));

        let received = rx.recv().await.unwrap();
        assert_eq!(received.0.as_str(), "signal.BTC");
        let received = rx.recv().await.unwrap();
        assert_eq!(received.0.as_str(), "system.control");
    }

    #[tokio::test]
    async fn test_default_subject() {
        let signal = make_signal("BTC", "BUY");
        assert_eq!(signal.default_subject(), "signal.BTC");

        let price = RatEvent::MarketPrice(MarketPriceEvent {
            symbol: "BTC".to_string(),
            price: 50000.0,
            exchange: "binance".to_string(),
            timestamp_micros: 0,
        });
        assert_eq!(price.default_subject(), "market.price.BTC");
    }

    #[tokio::test]
    async fn test_subjects_module() {
        assert_eq!(subjects::all_signal_events().as_str(), "signal.>");
        assert_eq!(subjects::market_price("BTC").as_str(), "market.price.BTC");
        assert_eq!(subjects::portfolio_snapshot().as_str(), "portfolio.snapshot");
        assert_eq!(subjects::health("orchestrator").as_str(), "health.orchestrator");
        assert_eq!(subjects::signal("BTC").as_str(), "signal.BTC");
        assert_eq!(subjects::system_control().as_str(), "system.control");
        assert_eq!(subjects::all().as_str(), ">");
    }

    #[test]
    fn test_published_count() {
        let bus = EventBus::new(16);
        assert_eq!(bus.published_count(), 0);
        bus.publish(Subject::new("test"), make_signal("T", "BUY"));
        assert_eq!(bus.published_count(), 1);
        bus.publish(Subject::new("test"), make_signal("T", "BUY"));
        assert_eq!(bus.published_count(), 2);
    }
}
