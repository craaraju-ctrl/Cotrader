//! Workflow Orchestration — Ties all components together.

pub mod trading_workflow;
pub mod research_workflow;
pub mod risk_workflow;

pub use trading_workflow::TradingWorkflow;
