//! rat-agents — 21 trading agents in human hierarchical structure.
//!
//! Modeled after real trading firms:
//!   Rat (CIO)
//!   ├── Head of Trading → Equity Trader, Crypto Trader, Execution Desk
//!   ├── Head of Research → Quant Researcher, Technical Analyst, Fundamental Analyst
//!   ├── Head of Risk → Market Risk Manager, Compliance Officer
//!   ├── Head of Operations → Portfolio Administrator, Journal Keeper
//!   └── Head of Technology → System Architect, Data Engineer, Backtest Engine

pub mod agents;
pub mod thinking;
pub mod traits;
pub mod processors;
