//! # Broker — Unified Broker Interface
//!
//! Re-exports everything from [`crate::paper_engine`] which provides:
//! - [`BrokerAdapter`] trait — Unified interface for ALL brokers
//! - [`BrokerRegistry`] — Routes orders between paper/live mode
//! - Shared types: [`OrderRequest`], [`OrderType`], [`OrderStatus`], [`Position`], etc.
//!
//! ## Usage
//! ```
//! use cotrader_core::paper_engine::*;
//! ```
//!
//! ## Paper/Live Parity
//! The exact same code path is used for both paper and live trading.
//! The only difference is which API endpoint the broker connects to.
//! - Alpaca paper: `AlpacaBroker::new(key, secret, true)` → `paper-api.alpaca.markets`
//! - Alpaca live:  `AlpacaBroker::new(key, secret, false)` → `api.alpaca.markets`

pub use crate::paper_engine::*;
