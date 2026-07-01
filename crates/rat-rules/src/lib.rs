//! rat-rules — Modular trading rules engine.
//!
//! Each rule is a separate file organized by priority:
//! - `critical/` — Never overridden, always block
//! - `high/` — Always block when triggered
//! - `medium/` — Block only if no higher rule overrides
//! - `low/` — Warnings only, never block

pub mod critical;
pub mod high;
pub mod medium;
pub mod low;
pub mod context;
pub mod rule;

pub use context::RuleContext;
pub use rule::{Rule, RuleResult};
