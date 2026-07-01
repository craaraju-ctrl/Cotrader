//! WebSocket Data Feed — Real-time streaming from exchanges.

pub mod websocket_feed;
pub mod rest_polling;

pub use websocket_feed::WebSocketFeed;
pub use rest_polling::RestPollingFeed;
