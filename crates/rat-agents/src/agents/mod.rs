//! 21 trading agents organized by human firm hierarchy.

// Level 0: CEO/CIO
pub mod rat;

// Level 1: Department Heads (5)
pub mod head_of_trading;
pub mod head_of_research;
pub mod head_of_risk;
pub mod head_of_operations;
// head_of_technology is system_architect

// Level 2: Specialists (15)
// Trading Desk
pub mod equity_trader;
pub mod crypto_trader;
pub mod execution_desk;

// Research Desk
pub mod quant_researcher;
pub mod technical_analyst;
pub mod fundamental_analyst;

// Risk Desk
pub mod market_risk_manager;
pub mod compliance_officer;

// Operations
pub mod portfolio_administrator;
pub mod journal_keeper;

// Technology
pub mod system_architect;
pub mod data_engineer;
pub mod backtest_engine;

// Cross-cutting
pub mod sentiment_analyst;
pub mod regime_detector;
pub mod money_manager;
