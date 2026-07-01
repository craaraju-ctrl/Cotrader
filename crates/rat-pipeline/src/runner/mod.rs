//! Pipeline Runner — Orchestrates all 21 agents through the decision flow.

pub mod pipeline;
pub mod event_bus;
pub mod signal_flow;
pub mod risk_flow;
pub mod execution_flow;
pub mod feedback_flow;
pub mod agents;

pub use pipeline::PipelineRunner;
