//! API Client — Background HTTP polling + WebSocket for orchestrator data.
//!
//! Spawns background threads that poll the orchestrator REST API and forward
//! parsed data to the main TUI event loop via `std::sync::mpsc` channels.
//!
//! Endpoints polled:
//! - `GET /api/summary`     → Portfolio summary (equity, cash, P&L, trades)
//! - `GET /api/positions`   → Open positions list
//! - `GET /api/health`      → Server health status
//! - `GET /api/trades`      → Recent trade history
//! - `GET /api/status`      → Trading mode, broker, open positions count
//! - `GET /api/mode`        → Current mode (paper/live) — also POST to toggle
//! - `GET /api/cot`         → Chain-of-thought log entries (not yet on server)
//! - `GET /api/prices/all`  → Live market prices (not yet on server)
//! - `GET /api/agents`      → Agent hierarchy tree (not yet on server)
//! - `GET /api/skills`      → Skill votes + aggregated signal (not yet on server)
//!
//! WebSocket (`/ws`) streams real-time price updates and initial state.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use reqwest::blocking::Client as BlockingClient;
use serde_json::Value;

use crate::app::{AgentStatus, Position, ServiceStatus};

/// Maximum number of COT log entries to keep in memory.
const MAX_COT_ENTRIES: usize = 200;

/// Maximum number of equity/P&L history points for trend charts.
const MAX_HISTORY_LEN: usize = 200;

/// Outgoing command to the server (sent from TUI → background thread → HTTP POST).
#[derive(Debug)]
pub enum StatusMsg {
    /// Toggle between paper and live trading mode.
    ToggleMode,
}

/// Messages sent from background threads to the main TUI loop.
#[derive(Debug)]
pub enum ApiMessage {
    /// Full portfolio snapshot (positions + equity + P&L).
    Portfolio(PortfolioData),
    /// Recent trade history from /api/trades.
    Trades(Vec<TradeData>),
    /// System health status (Kronos, LLM, Orchestrator).
    Health(HealthData),
    /// COT log entries from the backend.
    CotEntries(Vec<CotEntry>),
    /// Live price updates for watchlist symbols.
    Prices(HashMap<String, PriceData>),
    /// Agent hierarchy with live status.
    Agents(Vec<AgentData>),
    /// Skill votes + aggregated signal.
    Skills(SkillData),
    /// WebSocket connection state changed.
    WsConnected(bool),
    /// WebSocket received a raw message (for COT/ping).
    WsMessage(String),
    /// Server status (mode, broker, open_positions, trading_enabled).
    Status(StatusData),
}

/// A single closed trade from `/api/trades`.
#[derive(Debug, Clone)]
pub struct TradeData {
    pub symbol: String,
    pub direction: String,
    pub qty: i32,
    pub entry_price: f64,
    pub exit_price: f64,
    pub realized_pnl: f64,
    pub closed_at: String,
}

/// Status data from `/api/status`.
#[derive(Debug, Clone, Default)]
pub struct StatusData {
    pub mode: String,
    pub broker: String,
    pub open_positions: u64,
    pub trading_enabled: bool,
    /// Error message from a failed command (e.g., mode toggle).
    pub error: Option<String>,
}

/// Portfolio data from `/api/portfolio`.
#[derive(Debug, Clone, Default)]
pub struct PortfolioData {
    pub total_equity: f64,
    pub cash_balance: f64,
    pub daily_pnl: f64,
    pub _daily_pnl_pct: f64,
    pub total_trades_today: u64,
    pub winning_trades_today: u64,
    pub losing_trades_today: u64,
    pub open_positions: Vec<PositionData>,
    pub max_drawdown_today: f64,
    pub _consecutive_losses: u32,
    pub _initial_balance: f64,
}

/// Single open position from the backend.
#[derive(Debug, Clone, Default)]
pub struct PositionData {
    pub symbol: String,
    pub direction: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_pct: f64,
    pub _stop_loss: f64,
    pub _take_profit: f64,
}

/// Health data from `/api/health`.
#[derive(Debug, Clone, Default)]
pub struct HealthData {
    pub kronos: bool,
    pub orchestrator: bool,
    pub llm: bool,
    pub model: String,
}

/// A single COT (chain-of-thought) log entry.
#[derive(Debug, Clone)]
pub struct CotEntry {
    pub agent: String,
    pub action: String,
    pub reason: String,
    pub confidence: f64,
    pub timestamp: String,
    pub symbol: Option<String>,
}

/// Parse a volume string like "1.5K" or "1.2M" into a numeric value.
fn parse_volume_str(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() || s == "—" {
        return 0.0;
    }
    let multiplier = if s.ends_with('M') {
        1_000_000.0
    } else if s.ends_with('K') {
        1_000.0
    } else if s.ends_with('B') {
        1_000_000_000.0
    } else {
        1.0
    };
    let num_str = s.trim_end_matches(|c: char| c.is_alphabetic());
    num_str.parse::<f64>().unwrap_or(0.0) * multiplier
}

/// Price data for a single symbol.
#[derive(Debug, Clone, Default)]
pub struct PriceData {
    pub price: f64,
    pub change_pct: f64,
    pub volume: String,
    pub _exchange: String,
}

/// Agent data from `/api/agents`.
#[derive(Debug, Clone)]
pub struct AgentData {
    pub name: String,
    pub tier: String,
    pub children: Vec<AgentData>,
}

/// Skill votes + aggregated signal.
#[derive(Debug, Clone, Default)]
pub struct SkillData {
    pub votes: Vec<SkillVote>,
    pub _aggregated: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct SkillVote {
    pub skill_name: String,
    pub score: f64,
    pub _weight: f64,
}

// ── Start background polling threads ────────────────────────────────────────

/// Start all background data-fetching threads. Returns a receiver for API
/// messages and a sender for outgoing commands (mode toggle, etc.).
///
/// Performs an immediate blocking fetch of all endpoints so the TUI has live
/// data from the very first render, then spawns background pollers to keep it
/// fresh.
///
/// The `api_base` should be the orchestrator HTTP URL (e.g., `http://localhost:8080/api`).
pub fn start_api_client(api_base: &str) -> (mpsc::Receiver<ApiMessage>, mpsc::Sender<StatusMsg>) {
    let (tx, rx) = mpsc::channel::<ApiMessage>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<StatusMsg>();

    let base = api_base.to_string();

    // ── Immediate blocking fetch (server-side) ──────────────────────────
    // Populates the channel so the first render already shows live data.
    fetch_initial_data(&base, &tx);

    // ── Background pollers ─────────────────────────────────────────────
    // 1. Portfolio + Health poller (every 3s)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || poll_portfolio_health(base, tx));
    }

    // 2. Price poller (every 5s)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || poll_prices(base, tx));
    }

    // 3. COT poller (every 10s)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || poll_cot(base, tx));
    }

    // 4. Agent tree poller (every 15s)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || poll_agents(base, tx));
    }

    // 5. Skills poller (every 8s)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || poll_skills(base, tx));
    }

    // 6. WebSocket listener (persistent connection)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || ws_listener(base, tx));
    }

    // 7. Status poller (every 10s)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || poll_status(base, tx));
    }

    // 8. Trade history poller (every 7s)
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || poll_trades(base, tx));
    }

    // 9. Command listener — drains outgoing commands with low latency
    {
        let tx = tx.clone();
        let base = base.clone();
        thread::spawn(move || command_listener(base, tx, cmd_rx));
    }

    (rx, cmd_tx)
}

/// Blocking initial fetch of all endpoints. Called synchronously before the
/// first render so the TUI starts with live data instead of blank screens.
///
/// All 6 endpoints are fetched in parallel via threads, so the worst-case
/// startup latency is the single-request timeout (2s), not the sum.
fn fetch_initial_data(base_url: &str, tx: &mpsc::Sender<ApiMessage>) {
    // Spawn a thread per endpoint, join them all so we block until done.
    // Uses the actual server endpoints: /api/summary, /api/positions, etc.
    let handles: Vec<_> = [
        ("summary",  format!("{}/summary",  base_url)),
        ("positions", format!("{}/positions", base_url)),
        ("health",   format!("{}/health",   base_url)),
        ("trades",   format!("{}/trades",   base_url)),
        ("status",   format!("{}/status",   base_url)),
    ]
    .into_iter()
    .map(|(name, url)| {
        let tx = tx.clone();
        let client = make_client();
        thread::spawn(move || {
            if let Ok(resp) = client.get(&url).send() {
                if let Ok(json) = resp.json::<Value>() {
                    // For portfolio, we need summary + positions together.
                    // summary and positions are fetched separately and combined
                    // in the poller, but for initial fetch we fetch both and
                    // send them as a single Portfolio message.
                    let msg = match name {
                        "health"   => Some(ApiMessage::Health(parse_health(&json))),
                        "trades"   => Some(ApiMessage::Trades(parse_trades(&json))),
                        "status"   => Some(ApiMessage::Status(parse_status(&json))),
                        _           => None,
                    };
                    if let Some(m) = msg {
                        let _ = tx.send(m);
                    }
                }
            }
        })
    })
    .collect();

    // Block until all initial fetches complete.
    for h in handles {
        let _ = h.join();
    }

    // Now fetch summary + positions and combine them into a Portfolio message.
    let client = make_client();
    let summary_url = format!("{}/summary", base_url);
    let positions_url = format!("{}/positions", base_url);
    let summary = client.get(&summary_url).send()
        .ok().and_then(|r| r.json::<Value>().ok());
    let positions = client.get(&positions_url).send()
        .ok().and_then(|r| r.json::<Value>().ok());
    if let (Some(s), Some(p)) = (summary, positions) {
        let data = parse_portfolio_from_summary(&s, &p);
        let _ = tx.send(ApiMessage::Portfolio(data));
    }
}

// ── HTTP Polling Functions ──────────────────────────────────────────────────

fn make_client() -> BlockingClient {
    BlockingClient::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}

fn poll_portfolio_health(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    let client = make_client();
    let summary_url = format!("{}/summary", base_url);
    let positions_url = format!("{}/positions", base_url);
    let health_url = format!("{}/health", base_url);

    loop {
        // Fetch portfolio summary + positions (server has these as separate endpoints)
        let summary = client.get(&summary_url).send()
            .ok().and_then(|r| r.json::<Value>().ok());
        let positions = client.get(&positions_url).send()
            .ok().and_then(|r| r.json::<Value>().ok());

        match (summary, positions) {
            (Some(s), Some(p)) => {
                let data = parse_portfolio_from_summary(&s, &p);
                let _ = tx.send(ApiMessage::Portfolio(data));
            }
            _ => {
                let _ = tx.send(ApiMessage::Portfolio(PortfolioData::default()));
            }
        }

        // Fetch health
        match client.get(&health_url).send() {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>() {
                    let data = parse_health(&json);
                    let _ = tx.send(ApiMessage::Health(data));
                }
            }
            Err(_) => {
                let _ = tx.send(ApiMessage::Health(HealthData::default()));
            }
        }

        thread::sleep(Duration::from_secs(3));
    }
}

fn poll_prices(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    let client = make_client();
    let url = format!("{}/prices/all", base_url);

    loop {
        match client.get(&url).send() {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>() {
                    let prices = parse_prices(&json);
                    let _ = tx.send(ApiMessage::Prices(prices));
                }
            }
            Err(_) => {}
        }
        thread::sleep(Duration::from_secs(5));
    }
}

fn poll_cot(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    let client = make_client();
    let url = format!("{}/cot", base_url);

    loop {
        match client.get(&url).send() {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>() {
                    let entries = parse_cot(&json);
                    let _ = tx.send(ApiMessage::CotEntries(entries));
                }
            }
            Err(_) => {}
        }
        thread::sleep(Duration::from_secs(10));
    }
}

fn poll_agents(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    let client = make_client();
    let url = format!("{}/agents", base_url);

    loop {
        match client.get(&url).send() {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>() {
                    let agents = parse_agent_tree(&json);
                    let _ = tx.send(ApiMessage::Agents(agents));
                }
            }
            Err(_) => {}
        }
        thread::sleep(Duration::from_secs(15));
    }
}

fn poll_skills(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    let client = make_client();
    let url = format!("{}/skills", base_url);

    loop {
        match client.get(&url).send() {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>() {
                    let skills = parse_skills(&json);
                    let _ = tx.send(ApiMessage::Skills(skills));
                }
            }
            Err(_) => {}
        }
        thread::sleep(Duration::from_secs(8));
    }
}

fn poll_trades(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    let client = make_client();
    let url = format!("{}/trades", base_url);

    loop {
        match client.get(&url).send() {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>() {
                    let trades = parse_trades(&json);
                    let _ = tx.send(ApiMessage::Trades(trades));
                }
            }
            Err(_) => {}
        }
        thread::sleep(Duration::from_secs(7));
    }
}

/// Poll `/api/status` every 10s.
fn poll_status(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    let client = make_client();
    let url = format!("{}/status", base_url);

    loop {
        match client.get(&url).send() {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>() {
                    let data = parse_status(&json);
                    let _ = tx.send(ApiMessage::Status(data));
                }
            }
            Err(_) => {}
        }

        thread::sleep(Duration::from_secs(10));
    }
}

/// Listen for outgoing commands with low latency (~100ms).
/// Processes StatusMsg commands by POSTing to the server and forwarding the
/// updated status back to the main loop.
fn command_listener(base_url: String, tx: mpsc::Sender<ApiMessage>, cmd_rx: mpsc::Receiver<StatusMsg>) {
    let client = make_client();
    let mode_url = format!("{}/mode", base_url);
    let status_url = format!("{}/status", base_url);

    loop {
        match cmd_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(StatusMsg::ToggleMode) => {
                // Fetch current mode, then POST the opposite
                let result = client.get(&mode_url).send()
                    .and_then(|r| r.json::<Value>())
                    .and_then(|json| {
                        let current = json.get("data")
                            .and_then(|d| d.get("mode"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("paper");
                        let new_mode = if current == "paper" { "live" } else { "paper" };
                        let body = serde_json::json!({ "mode": new_mode });
                        client.post(&mode_url)
                            .header("Content-Type", "application/json")
                            .body(body.to_string())
                            .send()
                    });

                match result {
                    Ok(_) => {
                        // Re-fetch status to confirm the change took effect
                        if let Ok(resp) = client.get(&status_url).send() {
                            if let Ok(json) = resp.json::<Value>() {
                                let data = parse_status(&json);
                                let _ = tx.send(ApiMessage::Status(data));
                            }
                        }
                    }
                    Err(e) => {
                        // Send error feedback without corrupting broker name
                        let _ = tx.send(ApiMessage::Status(StatusData {
                            error: Some(format!("Toggle failed: {}", e)),
                            ..Default::default()
                        }));
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No command pending — loop again after 100ms
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Command channel closed — exit
                break;
            }
        }
    }
}

// ── WebSocket Listener ──────────────────────────────────────────────────────

fn ws_listener(base_url: String, tx: mpsc::Sender<ApiMessage>) {
    // Convert http:// to ws://
    let ws_url = base_url
        .strip_suffix("/api")
        .unwrap_or(&base_url)
        .replace("http://", "ws://")
        .replace("https://", "wss://")
        .to_string() + "/ws";

    let mut retry_delay = Duration::from_secs(1);

    loop {
        match tungstenite::connect(&ws_url) {
            Ok((mut socket, _)) => {
                let _ = tx.send(ApiMessage::WsConnected(true));
                retry_delay = Duration::from_secs(1);

                loop {
                    match socket.read() {
                        Ok(tungstenite::Message::Text(text)) => {
                            // Forward raw JSON to main thread for processing
                            let _ = tx.send(ApiMessage::WsMessage(text.to_string()));
                        }
                        Ok(tungstenite::Message::Close(_)) => break,
                        Err(_) => break,
                        _ => {}
                    }
                }
                let _ = tx.send(ApiMessage::WsConnected(false));
            }
            Err(_) => {
                let _ = tx.send(ApiMessage::WsConnected(false));
            }
        }

        thread::sleep(retry_delay);
        retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
    }
}

// ── JSON Parsers ────────────────────────────────────────────────────────────

/// Parse portfolio from the actual server endpoints:
/// - `/api/summary` returns `{ success, data: { cash, equity, margin_used, daily_pnl, total_trades, ... } }`
/// - `/api/positions` returns `{ success, data: [...] }` (array of Position structs)
fn parse_portfolio_from_summary(summary: &Value, positions_json: &Value) -> PortfolioData {
    // Handle ApiResponse wrapper: { success: true, data: { ... } }
    let s = summary.get("data").unwrap_or(summary);
    let total_equity = s.get("equity").and_then(|v| v.as_f64()).unwrap_or(100_000.0);
    let cash_balance = s.get("cash").and_then(|v| v.as_f64()).unwrap_or(100_000.0);
    let daily_pnl = s.get("daily_pnl").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let total_trades = s.get("total_trades").and_then(|v| v.as_u64()).unwrap_or(0);
    let winning = s.get("winning_trades").and_then(|v| v.as_u64()).unwrap_or(0);
    let losing = s.get("losing_trades").and_then(|v| v.as_u64()).unwrap_or(0);
    let max_dd = s.get("max_drawdown_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
    // Parse positions from the separate /api/positions response
    let pos_data = positions_json.get("data")
        .and_then(|d| d.as_array())
        .or_else(|| positions_json.as_array());

    let open_positions: Vec<PositionData> = pos_data.map(|arr| {
        arr.iter().map(|p| PositionData {
            symbol: p.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            direction: p.get("direction").and_then(|v| v.as_str()).unwrap_or("Long").to_string(),
            quantity: p.get("qty").and_then(|v| v.as_f64()).unwrap_or(0.0),
            entry_price: p.get("entry_price").and_then(|v| v.as_f64()).unwrap_or(0.0),
            current_price: p.get("current_price").and_then(|v| v.as_f64()).unwrap_or(0.0),
            unrealized_pnl: p.get("unrealized_pnl").and_then(|v| v.as_f64()).unwrap_or(0.0),
            unrealized_pnl_pct: p.get("unrealized_pnl_pct").and_then(|v| v.as_f64()).unwrap_or(0.0),
            _stop_loss: p.get("stop_loss").and_then(|v| v.as_f64()).unwrap_or(0.0),
            _take_profit: p.get("take_profit").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }).collect()
    }).unwrap_or_default();

    PortfolioData {
        total_equity,
        cash_balance,
        daily_pnl,
        total_trades_today: total_trades,
        winning_trades_today: winning,
        losing_trades_today: losing,
        open_positions,
        max_drawdown_today: max_dd,
        ..Default::default()
    }
}

fn parse_health(json: &Value) -> HealthData {
    // Server returns: { success, data: { broker, mode, open_positions, status } }
    // No kronos/orchestrator/llm booleans — derive from the response itself.
    // If the server responds at all, the orchestrator is running.
    let data = json.get("data").unwrap_or(json);
    let status_ok = data.get("status").and_then(|v| v.as_str()) == Some("ok");
    let has_mode = data.get("mode").and_then(|v| v.as_str()).is_some();
    HealthData {
        kronos: status_ok,
        orchestrator: has_mode,
        llm: false, // No LLM health info from this endpoint
        model: data.get("broker")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
    }
}

fn parse_cot(json: &Value) -> Vec<CotEntry> {
    // The COT endpoint returns an array of entries
    let entries = if let Some(arr) = json.as_array() {
        arr
    } else if let Some(arr) = json.get("entries").and_then(|v| v.as_array()) {
        arr
    } else {
        return Vec::new();
    };

    entries
        .iter()
        .rev() // Most recent first
        .take(MAX_COT_ENTRIES)
        .map(|e| CotEntry {
            agent: e.get("agent").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
            action: e.get("action").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
            reason: e.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            confidence: e.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0),
            timestamp: e.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            symbol: e.get("symbol").and_then(|v| v.as_str()).map(|s| s.to_string()),
        })
        .collect()
}

fn parse_prices(json: &Value) -> HashMap<String, PriceData> {
    let mut prices = HashMap::new();

    if let Some(obj) = json.as_object() {
        for (sym, val) in obj {
            if let Some(data) = val.as_object() {
                prices.insert(
                    sym.clone(),
                    PriceData {
                        price: data.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        change_pct: data.get("change_pct").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        volume: data
                            .get("volume")
                            .and_then(|v| v.as_str())
                            .unwrap_or("—")
                            .to_string(),
                        _exchange: data
                            .get("exchange")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    },
                );
            }
        }
    }

    prices
}

fn parse_agent_tree(json: &Value) -> Vec<AgentData> {
    // The /api/agents returns the Rat tree as JSON
    // It's typically: { "name": "Rat", "tier": "orchestrator", "children": [...] }
    // We flatten it to a list for display
    let mut agents = Vec::new();

    fn flatten(node: &Value, agents: &mut Vec<AgentData>) {
        if let Some(name) = node.get("name").and_then(|v| v.as_str()) {
            let tier = node
                .get("tier")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let children: Vec<AgentData> = node
                .get("children")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|c| {
                            let mut child_agents = Vec::new();
                            flatten(c, &mut child_agents);
                            child_agents
                        })
                        .flatten()
                        .collect()
                })
                .unwrap_or_default();

            agents.push(AgentData {
                name: name.to_string(),
                tier,
                children,
            });

            // Also recurse into children to build flat list
            if let Some(arr) = node.get("children").and_then(|v| v.as_array()) {
                for child in arr {
                    flatten(child, agents);
                }
            }
        }
    }

    // Handle the tree root
    if json.is_object() {
        flatten(json, &mut agents);
    } else if let Some(arr) = json.as_array() {
        for node in arr {
            flatten(node, &mut agents);
        }
    }

    agents
}

fn parse_trades(json: &Value) -> Vec<TradeData> {
    // Server returns ApiResponse { success, data: [...trades] }
    let arr = json.get("data")
        .and_then(|d| d.as_array())
        .or_else(|| json.as_array());

    arr.map(|trades| {
        trades.iter().map(|t| TradeData {
            symbol: t.get("symbol").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
            direction: t.get("direction").and_then(|v| v.as_str())
                .or_else(|| t.get("side").and_then(|v| v.as_str()))
                .unwrap_or("?").to_string(),
            qty: t.get("qty").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            entry_price: t.get("entry_price").and_then(|v| v.as_f64())
                .or_else(|| t.get("price").and_then(|v| v.as_f64()))
                .unwrap_or(0.0),
            exit_price: t.get("exit_price").and_then(|v| v.as_f64()).unwrap_or(0.0),
            realized_pnl: t.get("realized_pnl").and_then(|v| v.as_f64()).unwrap_or(0.0),
            closed_at: t.get("closed_at").and_then(|v| v.as_str())
                .or_else(|| t.get("timestamp").and_then(|v| v.as_str()))
                .unwrap_or("?").to_string(),
        }).collect()
    }).unwrap_or_default()
}

fn parse_status(json: &Value) -> StatusData {
    // Handle ApiResponse wrapper: { success: true, data: { mode: ... } }
    let data = json.get("data").unwrap_or(json);
    StatusData {
        mode: data.get("mode").and_then(|v| v.as_str()).unwrap_or("paper").to_string(),
        broker: data.get("broker").and_then(|v| v.as_str()).unwrap_or("paper").to_string(),
        open_positions: data.get("open_positions").and_then(|v| v.as_u64()).unwrap_or(0),
        trading_enabled: data.get("trading_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        error: None,
    }
}

fn parse_skills(json: &Value) -> SkillData {
    let votes = json
        .get("votes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| SkillVote {
                    skill_name: v.get("skill_name").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                    score: v.get("score").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    _weight: v.get("weight").and_then(|v| v.as_f64()).unwrap_or(0.1),
                })
                .collect()
        })
        .unwrap_or_default();

    let aggregated = json.get("aggregated").cloned();

    SkillData { votes, _aggregated: aggregated }
}

// ── Message Processing (called from main loop) ──────────────────────────────

/// Process a single API message and update the App state accordingly.
pub fn process_message(msg: ApiMessage, app: &mut crate::app::App) {
    match msg {
        ApiMessage::Portfolio(data) => {
            // Update portfolio summary
            app.portfolio.equity = data.total_equity;
            app.portfolio.cash = data.cash_balance;
            app.portfolio.unrealized_pnl = data.open_positions.iter().map(|p| p.unrealized_pnl).sum();
            app.portfolio.realized_pnl = data.daily_pnl;
            app.portfolio.total_trades = data.total_trades_today;
            app.portfolio.winning_trades = data.winning_trades_today;
            app.portfolio.losing_trades = data.losing_trades_today;
            app.portfolio.win_rate = if data.total_trades_today > 0 {
                data.winning_trades_today as f64 / data.total_trades_today as f64 * 100.0
            } else {
                0.0
            };
            app.portfolio.max_drawdown = data.max_drawdown_today * 100.0;

            // Compute Sharpe ratio from equity history
            if app.equity_history.len() >= 3 {
                let returns: Vec<f64> = app.equity_history.windows(2)
                    .map(|w| if w[0] > 0.0 { (w[1] - w[0]) / w[0] } else { 0.0 })
                    .collect();
                let n = returns.len() as f64;
                let mean = returns.iter().sum::<f64>() / n;
                let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
                let std_dev = variance.sqrt();
                // Annualize: 3s intervals → ~10M periods/year → sqrt(10M) ≈ 3162
                app.portfolio.sharpe_ratio = if std_dev > 0.0 {
                    (mean / std_dev) * 3162.0
                } else {
                    0.0
                };
            }

            // Append equity and P&L history for trend charts
            let total_pnl = app.portfolio.unrealized_pnl + app.portfolio.realized_pnl;
            app.equity_history.push(data.total_equity);
            app.pnl_history.push(total_pnl);
            while app.equity_history.len() > MAX_HISTORY_LEN {
                app.equity_history.remove(0);
            }
            while app.pnl_history.len() > MAX_HISTORY_LEN {
                app.pnl_history.remove(0);
            }

            // Update positions
            app.positions = data
                .open_positions
                .iter()
                .map(|p| {
                    let leverage = if p.entry_price > 0.0 && p.quantity > 0.0 {
                        (p.quantity * p.entry_price / (data.total_equity - data.cash_balance).max(1.0)) as u32
                    } else {
                        1
                    };
                    Position {
                        symbol: p.symbol.clone(),
                        side: if p.direction == "Long" { "Long".to_string() } else { "Short".to_string() },
                        size: p.quantity,
                        entry_price: p.entry_price,
                        mark_price: p.current_price,
                        pnl: p.unrealized_pnl,
                        pnl_pct: p.unrealized_pnl_pct * 100.0,
                        leverage: leverage.max(1),
                        liquidation_price: if p.direction == "Long" {
                            p.entry_price * 0.9
                        } else {
                            p.entry_price * 1.1
                        },
                    }
                })
                .collect();
        }

        ApiMessage::Health(data) => {
            // Update service health map
            app.health.services.clear();

            app.health.services.insert(
                "Kronos".to_string(),
                ServiceStatus {
                    name: "Kronos".to_string(),
                    status: if data.kronos { "healthy".to_string() } else { "down".to_string() },
                    latency_ms: 0.0,
                    last_check: Some(std::time::Instant::now()),
                },
            );
            app.health.services.insert(
                "Orchestrator".to_string(),
                ServiceStatus {
                    name: "Orchestrator".to_string(),
                    status: if data.orchestrator { "running".to_string() } else { "down".to_string() },
                    latency_ms: 0.0,
                    last_check: Some(std::time::Instant::now()),
                },
            );
            app.health.services.insert(
                "LLM".to_string(),
                ServiceStatus {
                    name: format!("LLM ({})", data.model),
                    status: if data.llm { "healthy".to_string() } else { "down".to_string() },
                    latency_ms: 0.0,
                    last_check: Some(std::time::Instant::now()),
                },
            );
            app.health.services.insert(
                "Metrics".to_string(),
                ServiceStatus {
                    name: "Metrics".to_string(),
                    status: "monitoring".to_string(),
                    latency_ms: 0.0,
                    last_check: Some(std::time::Instant::now()),
                },
            );
            app.health.services.insert(
                "WebSocket".to_string(),
                ServiceStatus {
                    name: "WebSocket".to_string(),
                    status: if app.ws_connected { "connected".to_string() } else { "disconnected".to_string() },
                    latency_ms: 0.0,
                    last_check: Some(std::time::Instant::now()),
                },
            );
        }

        ApiMessage::CotEntries(entries) => {
            // Convert COT entries to JSON values for the existing COT log display
            app.cot_log.clear();
            for entry in entries {
                let json = serde_json::json!({
                    "agent": entry.agent,
                    "action": entry.action,
                    "reason": entry.reason,
                    "confidence": entry.confidence,
                    "timestamp": entry.timestamp,
                    "symbol": entry.symbol,
                });
                app.cot_log.push_back(json);
            }
        }

        ApiMessage::Prices(prices) => {
            // Update market data for all symbols in the watchlist
            for sym in &app.watchlist {
                if let Some(price_data) = prices.get(sym) {
                    let md = app.market_data.entry(sym.clone()).or_default();
                    md.symbol = sym.clone();
                    md.price = price_data.price;
                    md.change_24h = price_data.change_pct;
                    // bid/ask are not provided by the prices/all endpoint; derive from price
                    md.bid = price_data.price * 0.9999;
                    md.ask = price_data.price * 1.0001;
                    md.spread = md.ask - md.bid;
                    // Parse volume string like "1.5K" or "1.2M"
                    md.volume = parse_volume_str(&price_data.volume);
                }
            }
        }

        ApiMessage::Agents(agents) => {
            // Convert flat agent list to App's AgentStatus
            app.agents = agents
                .iter()
                .map(|a| AgentStatus {
                    name: a.name.clone(),
                    status: "active".to_string(),
                    confidence: 0.5, // Default confidence
                    last_action: a.tier.clone(),
                    reason: format!("{} agents in subtree", a.children.len()),
                })
                .collect();
        }

        ApiMessage::Skills(skills) => {
            // Update agent status with skill information if available
            if !skills.votes.is_empty() {
                // Enhance agent list with skill vote data
                for agent in &mut app.agents {
                    if let Some(vote) = skills.votes.iter().find(|v| v.skill_name == agent.name) {
                        agent.confidence = vote.score;
                        agent.status = if vote.score > 0.6 {
                            "active".to_string()
                        } else if vote.score < 0.4 {
                            "warning".to_string()
                        } else {
                            "active".to_string()
                        };
                    }
                }
            }
        }

        ApiMessage::Trades(trades) => {
            // Convert to JSON values for the Trading tab's open_orders display
            app.open_orders = trades.iter().map(|t| {
                serde_json::json!({
                    "id": t.closed_at,
                    "symbol": t.symbol,
                    "side": t.direction,
                    "type": "closed",
                    "quantity": t.qty,
                    "price": t.entry_price,
                    "status": "filled",
                    "exit_price": t.exit_price,
                    "realized_pnl": t.realized_pnl,
                })
            }).collect();

        }

        ApiMessage::Status(data) => {
            // On error, only set the error field; don't overwrite live data
            if let Some(err) = data.error {
                app.error = Some(err);
            } else {
                app.trading_mode = data.mode;
                app.broker_name = data.broker;
                app.trading_enabled = data.trading_enabled;
                app.open_position_count = data.open_positions;
                app.error = None; // Clear any previous error on success
            }
        }

        ApiMessage::WsConnected(connected) => {
            app.ws_connected = connected;
        }

        ApiMessage::WsMessage(raw) => {
            // Process WebSocket messages (price updates, COT entries, portfolio changes)
            if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                match json.get("type").and_then(|v| v.as_str()) {
                    Some("price") => {
                        // Real-time price update: { "type": "price", "symbol": "BTC", "price": 60000.0 }
                        if let (Some(sym), Some(price)) = (
                            json.get("symbol").and_then(|v| v.as_str()),
                            json.get("price").and_then(|v| v.as_f64()),
                        ) {
                            let md = app.market_data.entry(sym.to_string()).or_default();
                            md.symbol = sym.to_string();
                            md.price = price;
                        }
                    }
                    Some("price_update") | Some("portfolio") => {
                        // Price/portfolio update from WS — update market data if price available
                        if let (Some(sym), Some(price)) = (
                            json.get("symbol").and_then(|v| v.as_str()),
                            json.get("price").and_then(|v| v.as_f64()),
                        ) {
                            let md = app.market_data.entry(sym.to_string()).or_default();
                            md.symbol = sym.to_string();
                            md.price = price;
                        }
                    }
                    Some("initial_state") => {
                        // Initial WS connection state — contains summary + positions
                        // Server sends: { type: "initial_state", summary: PortfolioSummary, positions: [...] }
                        if let Some(summary) = json.get("summary") {
                            let s = summary; // PortfolioSummary struct fields
                            let mut data = PortfolioData {
                                total_equity: s.get("equity").and_then(|v| v.as_f64()).unwrap_or(100_000.0),
                                cash_balance: s.get("cash").and_then(|v| v.as_f64()).unwrap_or(100_000.0),
                                daily_pnl: s.get("daily_pnl").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                total_trades_today: s.get("total_trades").and_then(|v| v.as_u64()).unwrap_or(0),
                                winning_trades_today: s.get("winning_trades").and_then(|v| v.as_u64()).unwrap_or(0),
                                losing_trades_today: s.get("losing_trades").and_then(|v| v.as_u64()).unwrap_or(0),
                                max_drawdown_today: s.get("max_drawdown_pct").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                ..Default::default()
                            };
                            // Parse open positions from initial_state
                            if let Some(positions) = json.get("positions").and_then(|v| v.as_array()) {
                                data.open_positions = positions.iter().map(|p| PositionData {
                                    symbol: p.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    direction: p.get("direction").and_then(|v| v.as_str()).unwrap_or("Long").to_string(),
                                    quantity: p.get("qty").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    entry_price: p.get("entry_price").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    current_price: p.get("current_price").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    unrealized_pnl: p.get("unrealized_pnl").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    ..Default::default()
                                }).collect();
                            }
                            process_message(ApiMessage::Portfolio(data), app);
                        }
                    }
                    Some("cot") => {
                        // Single COT entry pushed via WS
                        if let Some(entry) = json.get("entry") {
                            let cot_entry = CotEntry {
                                agent: entry.get("agent").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                                action: entry.get("action").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                                reason: entry.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                confidence: entry.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                timestamp: entry.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                symbol: entry.get("symbol").and_then(|v| v.as_str()).map(|s| s.to_string()),
                            };
                            let json_val = serde_json::json!({
                                "agent": cot_entry.agent,
                                "action": cot_entry.action,
                                "reason": cot_entry.reason,
                                "confidence": cot_entry.confidence,
                                "timestamp": cot_entry.timestamp,
                                "symbol": cot_entry.symbol,
                            });
                            app.cot_log.push_front(json_val);
                            // Trim to max size
                            while app.cot_log.len() > MAX_COT_ENTRIES {
                                app.cot_log.pop_back();
                            }
                        }
                    }
                    Some("signal") => {
                        // Aggregated signal update
                        if let Some(sig) = json.get("aggregated") {
                            if let Some(first_agent) = app.agents.first_mut() {
                                let action = sig.get("action").and_then(|v| v.as_str()).unwrap_or("HOLD");
                                let conf = sig.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                first_agent.last_action = action.to_string();
                                first_agent.confidence = conf;
                            }
                        }
                    }
                    Some("pipeline_event") => {
                        // Pipeline lifecycle event from EventBus bridge.
                        // Contains the full signal data (action, symbol, prices, confidence, reasoning).
                        // Populate BOTH the COT log (for Agents tab) AND the dedicated pipeline panel.
                        let action = json.get("action").and_then(|v| v.as_str()).unwrap_or("HOLD").to_string();
                        let symbol = json.get("symbol").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                        let confidence = json.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let reasoning = json.get("reasoning").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let source = json.get("source").and_then(|v| v.as_str()).unwrap_or("pipeline").to_string();
                        let ts = json.get("timestamp").and_then(|v| v.as_i64())
                            .map(|ts| ts.to_string())
                            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

                        // 1. Push to dedicated pipeline events panel
                        let pipeline_entry = crate::app::PipelineEvent {
                            symbol: symbol.clone(),
                            action: action.clone(),
                            confidence,
                            reasoning: reasoning.clone(),
                            source: source.clone(),
                            timestamp: ts.clone(),
                        };
                        app.pipeline_events.push_front(pipeline_entry);
                        while app.pipeline_events.len() > 50 {
                            app.pipeline_events.pop_back();
                        }

                        // 2. Push to COT log for Agents tab visibility
                        let cot_entry = serde_json::json!({
                            "agent": format!("Pipeline ({})", source),
                            "action": action,
                            "reason": reasoning,
                            "confidence": confidence,
                            "timestamp": ts,
                            "symbol": symbol,
                        });
                        app.cot_log.push_front(cot_entry);
                        while app.cot_log.len() > MAX_COT_ENTRIES {
                            app.cot_log.pop_back();
                        }

                        // 3. Update agent status
                        if let Some(first_agent) = app.agents.first_mut() {
                            first_agent.last_action = action.clone();
                            first_agent.confidence = confidence;
                            first_agent.status = if action == "HOLD" {
                                "observing".to_string()
                            } else {
                                "trading".to_string()
                            };
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
