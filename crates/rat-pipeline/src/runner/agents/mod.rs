//! Agent Integration — Connects 21 agents to the pipeline.

pub mod trading_desk;
pub mod research_desk;
pub mod risk_desk;
pub mod operations;
pub mod technology;

pub use trading_desk::TradingDesk;
pub use research_desk::ResearchDesk;
pub use risk_desk::RiskDesk;
pub use operations::Operations;
pub use technology::Technology;
