use std::collections::{HashMap, VecDeque};
use chrono::{DateTime, Local};

pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
pub const NUM_TABS: usize = 10;

/// Alert severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
    Trade,
}

impl AlertLevel {
    pub fn icon(&self) -> &str {
        match self {
            AlertLevel::Info => "ℹ",
            AlertLevel::Warning => "⚠",
            AlertLevel::Critical => "🔴",
            AlertLevel::Trade => "💹",
        }
    }
}

/// A single pipeline lifecycle event (from EventBus bridge).
#[derive(Debug, Clone)]
pub struct PipelineEvent {
    pub symbol: String,
    pub action: String,
    pub confidence: f64,
    pub reasoning: String,
    pub source: String,
    pub timestamp: String,
}

/// A single alert/notification
#[derive(Debug, Clone)]
pub struct Alert {
    pub level: AlertLevel,
    pub title: String,
    pub message: String,
    pub timestamp: DateTime<Local>,
    pub read: bool,
}

/// Portfolio risk metrics (computed each tick)
#[derive(Debug, Default, Clone)]
pub struct RiskMetrics {
    pub var_95: f64,
    pub var_99: f64,
    pub portfolio_beta: f64,
    pub sharpe_ratio: f64,
    pub concentration_pct: f64,
    pub total_exposure: f64,
    pub exposure_pct: f64,
    pub at_risk_positions: usize,
    pub daily_volatility: f64,
    pub margin_usage: f64,
}

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
    pub selected_symbol_idx: usize,
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

    // Config (from /api/status)
    pub trading_mode: String,
    pub broker_name: String,
    pub trading_enabled: bool,
    pub open_position_count: u64,

    // UI state
    pub ticker_offset: usize,
    pub focused_panel: usize,
    pub action_running: bool,
    pub action_message: Option<(String, std::time::Instant)>,

        // ── Pipeline lifecycle events (from EventBus bridge) ────────────────
    /// Recent pipeline lifecycle events for the dedicated pipeline panel.
    /// Pushed via `pipeline_event` WebSocket messages. Max 50 kept.
    pub pipeline_events: VecDeque<PipelineEvent>,

    // ── Advanced TUI features ──────────────────────────────────────────────
    /// Command palette open state
    pub show_command_palette: bool,
    pub command_palette_query: String,
    pub command_palette_selected: usize,
    pub pending_command: Option<crate::api_client::StatusMsg>,
    /// Real-time alerts/notifications
    pub alerts: VecDeque<Alert>,
    /// Risk metrics (computed from portfolio data)
    pub risk: RiskMetrics,
    /// Position detail view (when user selects a position)
    pub show_position_detail: bool,
    pub selected_position_idx: usize,
}

impl App {
    pub fn new() -> Self {
        let mut state = Self::default();
        state.pipeline_events = VecDeque::new();
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

    /// Add a new alert to the notification queue
    pub fn add_alert(&mut self, level: AlertLevel, title: String, message: String) {
        self.alerts.push_front(Alert {
            level,
            title,
            message,
            timestamp: chrono::Local::now(),
            read: false,
        });
        // Cap at 50 alerts
        while self.alerts.len() > 50 {
            self.alerts.pop_back();
        }
    }

    /// Recompute risk metrics from current portfolio state
    pub fn recompute_risk(&mut self) {
        let eq = self.portfolio.equity;
        let cash = self.portfolio.cash;
        let positions_value: f64 = self.positions.iter().map(|p| p.size * p.mark_price).sum();
        let total_exposure = positions_value;

        // Portfolio beta (weighted average of position betas, simplified)
        let portfolio_beta = if total_exposure > 0.0 {
            self.positions.iter().map(|p| {
                let weight = (p.size * p.mark_price) / total_exposure;
                weight * 1.0 // Simplified: assume beta=1 for all positions
            }).sum::<f64>()
        } else {
            0.0
        };

        // Value at Risk (95% confidence, simplified parametric)
        // VaR = portfolio_value * volatility * z_score * sqrt(horizon)
        let volatility = if self.equity_history.len() >= 10 {
            let returns: Vec<f64> = self.equity_history.windows(2)
                .map(|w| if w[0] > 0.0 { (w[1] - w[0]) / w[0] } else { 0.0 })
                .collect();
            let n = returns.len() as f64;
            if n > 0.0 {
                let mean = returns.iter().sum::<f64>() / n;
                let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
                variance.sqrt()
            } else {
                0.02
            }
        } else {
            0.02 // Default 2% daily volatility
        };
        let var_95 = eq * volatility * 1.645;
        let var_99 = eq * volatility * 2.326;

        // Concentration (largest position as % of portfolio)
        let max_position_value = self.positions.iter()
            .map(|p| (p.size * p.mark_price).abs())
            .fold(0.0_f64, f64::max);
        let concentration = if eq > 0.0 { max_position_value / eq * 100.0 } else { 0.0 };

        // Liquidation risk
        let at_risk_positions = self.positions.iter()
            .filter(|p| {
                if p.side == "Long" {
                    p.mark_price < p.liquidation_price * 1.1
                } else {
                    p.mark_price > p.liquidation_price * 0.9
                }
            }).count();

        self.risk = RiskMetrics {
            var_95,
            var_99,
            portfolio_beta,
            sharpe_ratio: self.portfolio.sharpe_ratio,
            concentration_pct: concentration,
            total_exposure,
            exposure_pct: if eq > 0.0 { total_exposure / eq * 100.0 } else { 0.0 },
            at_risk_positions,
            daily_volatility: volatility * 100.0,
            margin_usage: if eq > 0.0 { (eq - cash) / eq * 100.0 } else { 0.0 },
        };
    }
}
