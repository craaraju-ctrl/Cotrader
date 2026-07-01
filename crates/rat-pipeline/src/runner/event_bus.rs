//! Event Bus — Inter-agent communication via broadcast channels.

use tokio::sync::broadcast;

pub struct EventBus {
    pub sender: broadcast::Sender<Event>,
    pub receiver: broadcast::Receiver<Event>,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub source: String,
    pub event_type: EventType,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub enum EventType {
    MarketData,
    SignalGenerated,
    RiskChecked,
    TradeExecuted,
    TradeOutcome,
    AgentDecision,
    MemoryStored,
    Alert,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = broadcast::channel(capacity);
        Self { sender, receiver }
    }

    pub fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
}

impl Event {
    pub fn new(source: &str, event_type: EventType, data: serde_json::Value) -> Self {
        Self {
            source: source.to_string(),
            event_type,
            data,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}
