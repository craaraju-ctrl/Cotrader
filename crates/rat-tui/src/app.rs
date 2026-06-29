//! Application state — Central state for the entire TUI.

use std::collections::{HashMap, VecDeque};

pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
pub const NUM_TABS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Dashboard = 0,
    Trading = 1,
    Orderbook = 2,
    Positions = 3,
    Agents = 4,
    Performance = 5,
    PolicyCache = 6,
    Health = 7,
    Settings = 8,
    Help = 9,
}

impl Default for Tab {
    fn default() -> Self {
        Tab::Dashboard
    }
}

impl Tab {
    pub fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => " Dashboard ",
            Tab::Trading => " Trading ",
            Tab::Orderbook => " Orderbook ",
            Tab::Positions => " Positions ",
            Tab::Agents => " Agents ",
            Tab::Performance => " Performance ",
            Tab::PolicyCache => " Policy ",
            Tab::Health => " Health ",
            Tab::Settings => " Settings ",
            Tab::Help => " Help ",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tab::Dashboard => "📊",
            Tab::Trading => "💹",
            Tab::Orderbook => "📖",
            Tab::Positions => "📋",
            Tab::Agents => "🤖",
            Tab::Performance => "📈",
            Tab::PolicyCache => "🧠",
            Tab::Health => "🔷",
            Tab::Settings => "⚙️",
            Tab::Help => "❓",
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MarketData {
    pub symbol: String,
    pub price: f64,
    pub change_24h: f64,
    pub volume: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub bid: f64,
    pub ask: f64,
    pub spread: f64,
}

#[derive(Debug, Default, Clone)]
pub struct OrderBookLevel {
    pub price: f64,
    pub quantity: f64,
    pub total: f64,
}

#[derive(Debug, Default, Clone)]
pub struct OrderBook {
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub spread: f64,
    pub mid_price: f64,
}

#[derive(Debug, Default, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub pnl: f64,
    pub pnl_pct: f64,
    pub leverage: u32,
    pub liquidation_price: f64,
}

#[derive(Debug, Default, Clone)]
pub struct PortfolioSummary {
    pub equity: f64,
    pub cash: f64,
    pub margin_used: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub win_rate: f64,
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
}

#[derive(Debug, Default, Clone)]
pub struct AgentStatus {
    pub name: String,
    pub status: String,
    pub confidence: f64,
    pub last_action: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub status: String,
    pub latency_ms: f64,
    pub last_check: Option<std::time::Instant>,
}

#[derive(Debug, Default, Clone)]
pub struct SystemHealth {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub uptime: String,
    pub ws_connected: bool,
    pub services: HashMap<String, ServiceStatus>,
}

#[derive(Debug, Default)]
pub struct App {
    pub selected_tab: Tab,
    pub scroll_offset: usize,
    pub selected_row: usize,
    pub show_help: bool,
    pub show_search: bool,
    pub search_query: String,
    pub search_active: bool,
    pub last_tick: Option<std::time::Instant>,
    pub ws_connected: bool,
    pub error: Option<String>,

    // Market data
    pub watchlist: Vec<String>,
    pub market_data: HashMap<String, MarketData>,
    pub selected_symbol: String,

    // Trading
    pub orderbook: OrderBook,
    pub positions: Vec<Position>,
    pub open_orders: Vec<serde_json::Value>,

    // Portfolio
    pub portfolio: PortfolioSummary,
    pub equity_history: Vec<f64>,
    pub pnl_history: Vec<f64>,

    // Agents
    pub agents: Vec<AgentStatus>,
    pub cot_log: VecDeque<serde_json::Value>,
    pub policy_cache: Vec<serde_json::Value>,

    // System
    pub health: SystemHealth,
    pub service_status: Option<serde_json::Value>,

    // UI state
    pub ticker_offset: usize,
    pub focused_panel: usize,
    pub action_running: bool,
    pub action_message: Option<(String, std::time::Instant)>,
}

impl App {
    pub fn new() -> Self {
        let mut state = Self::default();
        state.watchlist = vec![
            "BTC".to_string(),
            "ETH".to_string(),
            "SOL".to_string(),
            "BNB".to_string(),
            "XRP".to_string(),
            "DOGE".to_string(),
            "ADA".to_string(),
            "AVAX".to_string(),
        ];
        state.selected_symbol = "BTC".to_string();
        state
    }

    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }
}
