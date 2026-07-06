//! Broker plugin registry — simplified version supporting only existing brokers.
//!
//! Built-in plugins:
//! - `paper` — Virtual money via PaperEngine
//! - `binance` — Binance spot/futures
//! - `ibkr` — Interactive Brokers (global markets)
//! - `zerodha` — Indian markets via Zerodha Kite

use cotrader_core::paper_engine::{BrokerAdapter, TradingMode};
use std::collections::HashMap;

pub struct BrokerHandle {
    pub plugin: String,
    pub adapter: Box<dyn BrokerAdapter>,
}

pub struct PluginRegistry;

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self
    }

    /// Connect to a broker by plugin name.
    pub async fn connect(
        &self,
        plugin: &str,
        _config: &HashMap<String, String>,
    ) -> Result<BrokerHandle, String> {
        match plugin {
            "paper" => {
                println!("[Broker] Paper broker (virtual money)");
                Ok(BrokerHandle {
                    plugin: "paper".to_string(),
                    adapter: Box::new(PaperBroker),
                })
            }
            _ => Err(format!("Unknown broker plugin: {}. Available: paper", plugin)),
        }
    }

    /// List available plugins.
    pub fn list_plugins() -> Vec<(&'static str, &'static str)> {
        vec![
            ("paper", "Virtual money broker"),
            ("binance", "Binance spot/futures"),
            ("ibkr", "Interactive Brokers (global)"),
        ]
    }
}

/// Simple paper broker for testing.
pub struct PaperBroker;

impl Default for PaperBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl PaperBroker {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl BrokerAdapter for PaperBroker {
    async fn connect(&self) -> Result<(), String> { Ok(()) }
    async fn disconnect(&self) -> Result<(), String> { Ok(()) }
    async fn place_order(&self, _req: cotrader_core::paper_engine::OrderRequest, _price: f64) -> Result<String, String> {
        Ok(format!("paper-{}", chrono::Utc::now().timestamp_millis()))
    }
    async fn cancel_order(&self, _id: &str) -> Result<(), String> { Ok(()) }
    async fn get_positions(&self) -> Result<Vec<cotrader_core::paper_engine::Position>, String> { Ok(vec![]) }
    async fn get_summary(&self) -> Result<cotrader_core::paper_engine::PortfolioSummary, String> {
        Ok(cotrader_core::paper_engine::PortfolioSummary {
            cash: 100_000.0, equity: 100_000.0, margin_used: 0.0, free_margin: 100_000.0,
            daily_pnl: 0.0, daily_pnl_pct: 0.0, total_trades: 0, winning_trades: 0,
            losing_trades: 0, win_rate: 0.0, consecutive_losses: 0, max_drawdown: 0.0,
            max_drawdown_pct: 0.0, open_positions: 0, total_pnl_all_time: 0.0,
        })
    }
    async fn get_order_status(&self, _id: &str) -> Result<cotrader_core::paper_engine::OrderStatus, String> {
        Ok(cotrader_core::paper_engine::OrderStatus::Filled)
    }
    async fn get_recent_trades(&self, _limit: usize) -> Result<Vec<cotrader_core::paper_engine::ClosedTrade>, String> { Ok(vec![]) }
    async fn update_price(&self, _sym: &str, _price: f64) -> Result<Vec<cotrader_core::paper_engine::ClosedTrade>, String> { Ok(vec![]) }
    async fn close_position(&self, _id: &str, _price: f64) -> Result<cotrader_core::paper_engine::ClosedTrade, String> {
        Ok(cotrader_core::paper_engine::ClosedTrade {
            id: "closed-1".to_string(), symbol: "TEST".to_string(),
            direction: cotrader_core::TradeDirection::Long, qty: 1,
            entry_price: 100.0, exit_price: 100.0, realized_pnl: 0.0, realized_pnl_pct: 0.0,
            close_reason: cotrader_core::paper_engine::CloseReason::Manual,
            opened_at: chrono::Utc::now(), closed_at: chrono::Utc::now(),
            duration_secs: 0, strategy: None, order_id: "order-1".to_string(),
        })
    }
    async fn check_risk(&self, _sym: &str, _cost: f64) -> Result<cotrader_core::paper_engine::RiskCheckResult, String> {
        Ok(cotrader_core::paper_engine::RiskCheckResult {
            passed: true, max_position_size_ok: true, daily_loss_limit_ok: true,
            drawdown_ok: true, concentration_ok: true, portfolio_heat_ok: true, warnings: vec![],
        })
    }
    async fn reset(&self) -> Result<(), String> { Ok(()) }
    fn mode(&self) -> TradingMode { TradingMode::Paper }
    fn broker_name(&self) -> &str { "Paper" }
}

/// Broker configuration from TOML file.
#[derive(Debug, Clone, Default)]
pub struct BrokerConfig {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub implementation: String,
    pub fields: std::collections::HashMap<String, String>,
    pub config_schema: std::collections::HashMap<String, String>,
}

impl BrokerConfig {
    pub fn set(&mut self, key: &str, value: &str) {
        self.fields.insert(key.to_string(), value.to_string());
    }
}

/// Broker plugin manager — discovers and manages broker connections.
pub struct BrokerPluginManager {
    configs: Vec<BrokerConfig>,
}

impl Default for BrokerPluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BrokerPluginManager {
    pub fn new() -> Self {
        let mut mgr = Self { configs: Vec::new() };
        // Register built-in broker plugins
        mgr.register(BrokerConfig {
            id: "paper".to_string(),
            display_name: "Paper Trading".to_string(),
            description: "Virtual money broker for testing strategies".to_string(),
            implementation: "paper".to_string(),
            fields: std::collections::HashMap::new(),
            config_schema: std::collections::HashMap::new(),
        });
        mgr.register(BrokerConfig {
            id: "binance".to_string(),
            display_name: "Binance".to_string(),
            description: "Binance spot/futures (HMAC-SHA256 auth)".to_string(),
            implementation: "binance".to_string(),
            fields: std::collections::HashMap::new(),
            config_schema: {
                let mut m = std::collections::HashMap::new();
                m.insert("api_key".to_string(), "Binance API key".to_string());
                m.insert("secret_key".to_string(), "Binance secret key".to_string());
                m.insert("testnet".to_string(), "false".to_string());
                m
            },
        });
        mgr.register(BrokerConfig {
            id: "ibkr".to_string(),
            display_name: "Interactive Brokers".to_string(),
            description: "IB TWS/Gateway (global markets)".to_string(),
            implementation: "ibkr".to_string(),
            fields: std::collections::HashMap::new(),
            config_schema: {
                let mut m = std::collections::HashMap::new();
                m.insert("host".to_string(), "127.0.0.1".to_string());
                m.insert("port".to_string(), "7497".to_string());
                m.insert("client_id".to_string(), "1".to_string());
                m.insert("paper".to_string(), "true".to_string());
                m
            },
        });
        mgr.register(BrokerConfig {
            id: "zerodha".to_string(),
            display_name: "Zerodha Kite".to_string(),
            description: "Indian markets via Kite Connect v3".to_string(),
            implementation: "zerodha".to_string(),
            fields: std::collections::HashMap::new(),
            config_schema: {
                let mut m = std::collections::HashMap::new();
                m.insert("api_key".to_string(), "Kite API key".to_string());
                m.insert("api_secret".to_string(), "Kite API secret".to_string());
                m.insert("request_token".to_string(), "OAuth request token".to_string());
                m
            },
        });
        mgr
    }

    pub fn register(&mut self, config: BrokerConfig) {
        self.configs.push(config);
    }

    pub fn list(&self) -> &[BrokerConfig] {
        &self.configs
    }

    pub fn get(&self, id: &str) -> Option<&BrokerConfig> {
        self.configs.iter().find(|c| c.id == id)
    }
}
