use crate::behavioral_psychology::BehavioralPsychologyEngine;
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::episode_store::EpisodeStore;
use crate::live_order_manager::LiveOrderManager;
use crate::types::{
    AgentDecision, CommunicationLog, CotEntry, MarketRegime, PortfolioState, TradeSignal,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use cotrader_core::paper_engine::BrokerRegistry;
use cotrader_core::memory_integration::MemoryIntegration;
use cotrader_core::{
    AdvancedPattern, CalendarEvent, Config, DisciplineRules, KnowledgeGraph, LlmExecutor,
    MemoryStore, NewsContext, OhlcvBar, PivotLevels, ServiceManager, SkillVote, TradingGoals,
    VectorMemory,
};
use cotrader_core::{CandlestickPattern, MultiTfPatternConfirmation};
use cotrader_ml::MLEngine;

/// Maximum COT entries kept in RAM before flushing to SQLite.
/// Reduced from 50 to 20 to keep RAM footprint smaller.
const MAX_COT_RAM: usize = 20;

/// Flush COT entries to SQLite only every N pushes (batch flush).
/// This reduces SQLite writes by 100× compared to flushing on every push.
const COT_FLUSH_INTERVAL: usize = 100;

/// Auto-prune COT entries older than this many days from SQLite.
const COT_PRUNE_DAYS: u64 = 7;

// ═══════════════════════════════════════════════════════════════════════
// Domain-Specific Stores
// ═══════════════════════════════════════════════════════════════════════

/// Portfolio & Risk — positions, P&L, broker, circuit breaker, psychology.
#[derive(Debug, Clone)]
pub struct PortfolioStore {
    pub portfolio: Arc<RwLock<PortfolioState>>,
    pub trading_goals: Arc<RwLock<TradingGoals>>,
    pub behavioral_psychology: Arc<RwLock<BehavioralPsychologyEngine>>,
    pub broker_registry: Arc<BrokerRegistry>,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub live_order_manager: Arc<LiveOrderManager>,
    pub memory_integration: Arc<MemoryIntegration>,
}

impl PortfolioStore {
    pub fn new(config: &Config, paper_broker: Arc<dyn cotrader_core::paper_engine::BrokerAdapter>) -> Self {
        Self {
            portfolio: Arc::new(RwLock::new(PortfolioState {
                cash_balance: config.initial_balance,
                total_equity: config.initial_balance,
                daily_pnl: 0.0,
                daily_pnl_pct: 0.0,
                open_positions: Vec::new(),
                total_trades_today: 0,
                winning_trades_today: 0,
                losing_trades_today: 0,
                consecutive_losses: 0,
                max_drawdown_today: 0.0,
                last_trade_time: None,
                last_trade_symbol: None,
                last_trade_by_symbol: HashMap::new(),
                trading_enabled: true,
            })),
            trading_goals: Arc::new(RwLock::new(TradingGoals::default())),
            behavioral_psychology: Arc::new(RwLock::new(BehavioralPsychologyEngine::new())),
            broker_registry: Arc::new(BrokerRegistry::new(paper_broker)),
            circuit_breaker: Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default())),
            live_order_manager: Arc::new(
                LiveOrderManager::open(Some("rat_orders.db")).unwrap_or_else(|e| {
                    eprintln!("[LiveOrderManager] ⚠ Failed to open order DB: {}", e);
                    LiveOrderManager::open(Some(":memory:")).expect("In-memory fallback failed")
                }),
            ),
            memory_integration: Arc::new(MemoryIntegration::new()),
        }
    }
}

/// Market Data — OHLCV, metrics, patterns, regimes, calendar, watchlist.
#[derive(Debug, Clone)]
pub struct MarketDataStore {
    pub ohlcv_history: Arc<RwLock<HashMap<String, Vec<OhlcvBar>>>>,
    pub market_regime: Arc<RwLock<Option<MarketRegime>>>,
    pub latest_metrics: Arc<RwLock<HashMap<String, crate::market_metrics_meter::MetricsSnapshot>>>,
    pub last_forecast: Arc<RwLock<Option<serde_json::Value>>>,
    pub multi_timeframe_data: Arc<RwLock<HashMap<String, Vec<TimeframeData>>>>,
    pub multi_tf_analyses: Arc<RwLock<HashMap<String, HashMap<String, TimeframeAnalysis>>>>,
    pub multi_tf_aggregate: Arc<RwLock<HashMap<String, MultiTfAggregate>>>,
    pub last_patterns: Arc<RwLock<HashMap<String, Vec<CandlestickPattern>>>>,
    pub last_mtf_patterns: Arc<RwLock<HashMap<String, MultiTfPatternConfirmation>>>,
    pub last_advanced_patterns: Arc<RwLock<HashMap<String, Vec<AdvancedPattern>>>>,
    pub last_tri_level_verdict:
        Arc<RwLock<HashMap<String, crate::tri_level_validator::TriLevelVerdict>>>,
    pub calendar_events: Arc<RwLock<Vec<CalendarEvent>>>,
    pub watchlist: Arc<RwLock<Vec<String>>>,
}

impl MarketDataStore {
    pub fn new() -> Self {
        Self {
            ohlcv_history: Arc::new(RwLock::new(HashMap::new())),
            market_regime: Arc::new(RwLock::new(None)),
            latest_metrics: Arc::new(RwLock::new(HashMap::new())),
            last_forecast: Arc::new(RwLock::new(None)),
            multi_timeframe_data: Arc::new(RwLock::new(HashMap::new())),
            multi_tf_analyses: Arc::new(RwLock::new(HashMap::new())),
            multi_tf_aggregate: Arc::new(RwLock::new(HashMap::new())),
            last_patterns: Arc::new(RwLock::new(HashMap::new())),
            last_mtf_patterns: Arc::new(RwLock::new(HashMap::new())),
            last_advanced_patterns: Arc::new(RwLock::new(HashMap::new())),
            last_tri_level_verdict: Arc::new(RwLock::new(HashMap::new())),
            calendar_events: Arc::new(RwLock::new(cotrader_core::generate_economic_calendar())),
            watchlist: Arc::new(RwLock::new(Self::default_watchlist())),
        }
    }

    fn default_watchlist() -> Vec<String> {
        if let Ok(env_watchlist) = std::env::var("WATCHLIST") {
            if !env_watchlist.trim().is_empty() {
                return env_watchlist
                    .split(',')
                    .map(|s| s.trim().to_uppercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        vec![
            // ── Layer1 / Smart Contract Platforms (24) ──
            "BTC","ETH","SOL","BNB","ADA","AVAX","DOT","MATIC","NEAR","ATOM",
            "FTM","ALGO","HBAR","ICP","XTZ","EGLD","FLOW","MINA","KSM","SEI","APT","INJ","SUI","TON","TRX",
            // ── DeFi / DEX / Lending (18) ──
            "UNI","AAVE","CRV","CAKE","SUSHI","COMP","MKR","SNX","BAL","YFI",
            "LDO","RPL","FXS","CVX","GMX","GNS","JOE","VELO",
            // ── Oracles / Infrastructure (6) ──
            "LINK","GRT","BAND","API3","TRB","UMA",
            // ── Payments / Currency / Privacy (7) ──
            "XRP","LTC","XLM","DASH","ZEC","XMR","NANO",
            // ── Gaming / Metaverse (10) ──
            "AXS","SAND","MANA","GALA","ENJ","CHZ","ILV","YGG","IMX","RON",
            // ── Meme / Community (8) ──
            "DOGE","SHIB","PEPE","WIF","BONK","FLOKI","BABYDOGE","ELON",
            // ── Layer2 / Scaling (6) ──
            "ARB","OP","LRC","BOBA","METIS","CTSI",
            // ── Storage / Compute / Data (4) ──
            "FIL","AR","STORJ","AKT",
            // ── Exchange / Platform Tokens (5) ──
            "CRO","OKB","KCS","LEO","HT",
            // ── AI / Data / Emerging (10) ──
            "FET","AGIX","OCEAN","RNDR","TAO","ARKM","NMR","TRAC","ORAI","MDT",
            // ── Stocks (US) (10) ──
            "AAPL","TSLA","NVDA","MSFT","AMZN","GOOGL","META","NFLX","AMD","INTC",
            // ── ETFs (2) ──
            "SPY","QQQ",
            // ── Stocks (India) (10) ──
            "RELIANCE","TCS","INFY","HDFCBANK","ICICIBANK","WIPRO","TATAMOTORS",
            "ADANIENT","BAJFINANCE","SBIN",
        ]
        .into_iter().map(String::from).collect()
    }
}

impl Default for MarketDataStore {
    fn default() -> Self { Self::new() }
}

/// Rule Engine — discipline rules, signals, LLM reasoning, layer weights.
#[derive(Debug, Clone)]
pub struct RuleEngine {
    pub rules: Arc<RwLock<DisciplineRules>>,
    pub last_signals: Arc<RwLock<Vec<TradeSignal>>>,
    pub layer_trust_weights: Arc<RwLock<crate::tri_level_validator::LayerTrustWeights>>,
    pub last_llm_reason: Arc<RwLock<String>>,
}

impl RuleEngine {
    pub fn new(rules: DisciplineRules) -> Self {
        Self {
            rules: Arc::new(RwLock::new(rules)),
            last_signals: Arc::new(RwLock::new(Vec::new())),
            layer_trust_weights: Arc::new(RwLock::new(
                crate::tri_level_validator::LayerTrustWeights::default(),
            )),
            last_llm_reason: Arc::new(RwLock::new(String::new())),
        }
    }
}

/// Agent Memory — episode store, vector memory, knowledge graph, COT.
#[derive(Debug, Clone)]
pub struct AgentMemoryStore {
    pub memory: Arc<MemoryStore>,
    pub episode_store: Arc<EpisodeStore>,
    pub vector_memory: Arc<tokio::sync::RwLock<cotrader_core::VectorMemory>>,
    pub knowledge_graph: Arc<RwLock<KnowledgeGraph>>,
    pub latest_episode: Arc<RwLock<HashMap<String, String>>>,
    pub latest_news: Arc<RwLock<HashMap<String, NewsContext>>>,
    pub last_skill_votes: Arc<RwLock<Vec<SkillVote>>>,
    pub last_aggregated_signal: Arc<RwLock<Option<cotrader_core::AggregatedSignal>>>,
    pub cot_store: Arc<RwLock<Vec<CotEntry>>>,
    pub cot_id_counter: Arc<AtomicU64>,
    pub agent_market_summary: Arc<RwLock<String>>,
}

impl AgentMemoryStore {
    pub fn new(memory: MemoryStore, episode_store: Arc<EpisodeStore>) -> Self {
        Self {
            memory: Arc::new(memory),
            episode_store,
            vector_memory: Arc::new(tokio::sync::RwLock::new(VectorMemory::new("rat_vectors.json"))),
            knowledge_graph: Arc::new(RwLock::new(KnowledgeGraph::new())),
            latest_episode: Arc::new(RwLock::new(HashMap::new())),
            latest_news: Arc::new(RwLock::new(HashMap::new())),
            last_skill_votes: Arc::new(RwLock::new(Vec::new())),
            last_aggregated_signal: Arc::new(RwLock::new(None)),
            cot_store: Arc::new(RwLock::new(Vec::new())),
            cot_id_counter: Arc::new(AtomicU64::new(1)),
            agent_market_summary: Arc::new(RwLock::new(String::new())),
        }
    }
}

/// IO & Infrastructure — broadcast channels, config, tasks.
#[derive(Debug, Clone)]
pub struct IoStore {
    pub update_tx: Arc<tokio::sync::broadcast::Sender<String>>,
    pub service_manager: Arc<cotrader_core::ServiceManager>,
    pub config: Arc<Config>,
    pub llm: Arc<LlmExecutor>,
    pub agent_tasks: Arc<RwLock<Vec<AgentTask>>>,
    pub last_watchlist_scan: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub communication_log: Arc<RwLock<CommunicationLog>>,
    pub pipeline_run_counter: Arc<AtomicU64>,
}

impl IoStore {
    pub fn new(config: &Config) -> Self {
        let (update_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            update_tx: Arc::new(update_tx),
            service_manager: Arc::new(ServiceManager::new()),
            config: Arc::new(config.clone()),
            llm: Arc::new(LlmExecutor::from_config(config)),
            agent_tasks: Arc::new(RwLock::new(vec![
                AgentTask::new("price_scan", 5),
                AgentTask::new("position_monitor", 10),
                AgentTask::new("market_scan", 300),
                AgentTask::new("portfolio_review", 3600),
                AgentTask::new("goal_review", 43200),
                AgentTask::new("daily_reflection", 86400),
            ])),
            last_watchlist_scan: Arc::new(RwLock::new(None)),
            communication_log: Arc::new(RwLock::new(CommunicationLog::new(0, ""))),
            pipeline_run_counter: Arc::new(AtomicU64::new(1)),
        }
    }
}

/// Multi-timeframe market data for a single symbol
#[derive(Debug, Clone)]
pub struct TimeframeData {
    pub timeframe: String, // "1m", "5m", "15m", "30m", "1h", "2h", "4h", "8h", "12h", "1d", "1w"
    pub ohlcv: Vec<OhlcvBar>,
    pub pivots: Option<PivotLevels>,
    pub confluence: f64,
    pub last_updated: DateTime<Utc>,
}

/// Per-timeframe complete analysis snapshot — indicators + patterns + skills
#[derive(Debug, Clone)]
pub struct TimeframeAnalysis {
    pub timeframe: String,
    pub metrics: crate::market_metrics_meter::MetricsSnapshot,
    pub patterns: Vec<CandlestickPattern>,
    pub confluence: f64,
    pub aggregated_direction: String, // "bullish" | "bearish" | "neutral"
    pub aggregated_conviction: f64,   // 0.0 to 1.0
    pub last_updated: DateTime<Utc>,
}

/// Aggregated multi-timeframe signal — weighted combination across all 11 TFs
#[derive(Debug, Clone)]
pub struct MultiTfAggregate {
    pub symbol: String,
    /// Per-timeframe analysis snapshots
    pub tf_analyses: HashMap<String, TimeframeAnalysis>,
    /// Number of timeframes with valid data
    pub tf_count: usize,
    /// Weighted aggregate signal (-1.0 to 1.0), where higher weights on longer TFs
    pub aggregate_signal: f64,
    /// Aggregate direction
    pub aggregate_direction: String,
    /// How many TFs agree with the aggregate direction (0.0 to 1.0)
    pub agreement_pct: f64,
    /// Confluence-weighted average score
    pub weighted_confluence: f64,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

/// A scheduled task for the agent to execute at specific intervals
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentTask {
    pub name: String,
    pub interval_secs: u64,
    pub last_run: Option<DateTime<Utc>>,
    pub enabled: bool,
}

impl AgentTask {
    pub fn new(name: &str, interval_secs: u64) -> Self {
        Self {
            name: name.to_string(),
            interval_secs,
            last_run: None,
            enabled: true,
        }
    }

    pub fn should_run(&self, now: &DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }
        match self.last_run {
            Some(last) => (*now - last).num_seconds() as u64 >= self.interval_secs,
            None => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SharedState {
    /// Portfolio & Risk — positions, P&L, broker, circuit breaker, psychology.
    pub portfolio_store: PortfolioStore,
    /// Market Data — OHLCV, metrics, patterns, regimes, calendar, watchlist.
    pub market_data: MarketDataStore,
    /// Rule Engine — discipline rules, signals, LLM reasoning, layer weights.
    pub rule_engine: RuleEngine,
    /// Agent Memory — episode store, vector memory, knowledge graph, COT.
    pub agent_memory: AgentMemoryStore,
    /// IO & Infrastructure — broadcast channels, event bus, config, tasks.
    pub io: IoStore,
    /// ML Engine — ML model inference (regime, signal scoring, win probability, patterns, strategy).
    pub ml_engine: Arc<MLEngine>,
}

impl SharedState {
    /// Get current skill weights (for FSM coordinator).
    pub fn get_skill_weights(&self) -> std::collections::HashMap<String, f64> {
        let mut weights = std::collections::HashMap::new();
        weights.insert("news_analyser".to_string(), 0.25);
        weights.insert("market_metrics_meter".to_string(), 0.25);
        weights.insert("sentiment_analyzer".to_string(), 0.25);
        weights.insert("on_chain_data".to_string(), 0.25);
        weights
    }

    /// Get current risk config (for FSM coordinator).
    pub fn get_risk_config(&self) -> crate::risk_guardian::RiskGuardianConfig {
        crate::risk_guardian::RiskGuardianConfig::default_fallback()
    }

    /// Create a new SharedState with an explicit paper broker adapter.
    pub fn new(
        memory: MemoryStore,
        rules: DisciplineRules,
        config: Config,
        db_path: &str,
        paper_broker: Arc<dyn cotrader_core::paper_engine::BrokerAdapter>,
    ) -> Result<Self, rusqlite::Error> {
        Self::with_event_bus(memory, rules, config, db_path, None, paper_broker)
    }

    pub fn with_event_bus(
        memory: MemoryStore,
        rules: DisciplineRules,
        config: Config,
        db_path: &str,
        _event_bus: Option<Arc<dyn std::any::Any + Send + Sync>>,
        paper_broker: Arc<dyn cotrader_core::paper_engine::BrokerAdapter>,
    ) -> Result<Self, rusqlite::Error> {
        let episode_store = Arc::new(EpisodeStore::open(db_path)?);

        let portfolio_store = PortfolioStore::new(&config, paper_broker);
        let market_data = MarketDataStore::new();
        let rule_engine = RuleEngine::new(rules);
        let agent_memory = AgentMemoryStore::new(memory, episode_store);
        let io = IoStore::new(&config);

        // Initialize ML engine — models loaded lazily on first inference
        let models_dir = std::path::PathBuf::from("data/models");
        let ml_engine = Arc::new(MLEngine::new(&models_dir));

        Ok(Self {
            portfolio_store,
            market_data,
            rule_engine,
            agent_memory,
            io,
            ml_engine,
        })
    }
}

impl SharedState {
    /// Refresh economic calendar from live API (FMP/AlphaVantage) or built-in fallback.
    pub async fn refresh_calendar(&self) {
        let events = cotrader_core::fetch_economic_calendar_live().await;
        *self.market_data.calendar_events.write().await = events;
    }

    /// Push a chain-of-thought entry into the store and return its unique ID.
    #[allow(clippy::too_many_arguments)]
    pub async fn push_cot(
        &self,
        agent: &str,
        input: &str,
        action: &str,
        reason: &str,
        confidence: f64,
        chain_id: u64,
        parent_id: Option<u64>,
        symbol: Option<String>,
    ) -> u64 {
        self.push_cot_with_persist(
            agent, input, action, reason, confidence, chain_id, parent_id, symbol, true,
        )
        .await
    }

    /// Push a COT entry with an explicit persist flag.
    /// When `persist` is false, the entry is broadcast via WebSocket for real-time TUI display
    /// but is NOT flushed to SQLite. This is useful for per-agent pipeline steps that are
    /// only relevant for real-time monitoring, not historical analysis.
    /// Only summary entries (with `persist=true`) get stored in SQLite.
    #[allow(clippy::too_many_arguments)]
    pub async fn push_cot_with_persist(
        &self,
        agent: &str,
        input: &str,
        action: &str,
        reason: &str,
        confidence: f64,
        chain_id: u64,
        parent_id: Option<u64>,
        symbol: Option<String>,
        persist: bool,
    ) -> u64 {
        let id = self.agent_memory.cot_id_counter.fetch_add(1, Ordering::Relaxed);
        let entry = CotEntry {
            id,
            chain_id,
            parent_id,
            agent: agent.to_string(),
            input: input.to_string(),
            action: action.to_string(),
            reason: reason.to_string(),
            confidence,
            timestamp: Utc::now().to_rfc3339(),
            symbol,
        };
        let mut store = self.agent_memory.cot_store.write().await;
        store.push(entry.clone());
        let store_len = store.len();

        // Only flush to SQLite when `persist` is true. Per-agent COT entries
        // (persist=false) are only kept in RAM for real-time TUI display and
        // are dropped when RAM is full — they never touch SQLite.
        // This is the key fix for COT explosion: 17 entries per pipeline run
        // → only 1 summary entry per run persists to SQLite.
        if persist && store_len > MAX_COT_RAM + COT_FLUSH_INTERVAL {
            let drain_count = store_len - MAX_COT_RAM;
            let overflow: Vec<_> = store.drain(0..drain_count).collect();
            let rows: Vec<crate::episode_store::CotLogRow> = overflow
                .iter()
                .map(|e| crate::episode_store::CotLogRow {
                    chain_id: e.chain_id,
                    agent: e.agent.clone(),
                    action: e.action.clone(),
                    reason: e.reason.clone(),
                    confidence: e.confidence,
                    symbol: e.symbol.clone(),
                    ts: e.timestamp.clone(),
                })
                .collect();
            let _ = self.agent_memory.episode_store.flush_cot_batch(&rows);
        }

        // Broadcast for WS real-time (connects to TUI/clients with debate/trained data)
        // Include all fields the COT renderer expects: timestamp, input, id, chain_id, etc.
        let update = serde_json::json!({
            "type": "cot",
            "id": entry.id,
            "chain_id": entry.chain_id,
            "parent_id": entry.parent_id,
            "agent": entry.agent,
            "input": entry.input,
            "action": entry.action,
            "reason": entry.reason,
            "confidence": entry.confidence,
            "timestamp": entry.timestamp,
            "symbol": entry.symbol
        })
        .to_string();
        let _ = self.io.update_tx.send(update);

        id
    }

    /// Prune old COT entries from SQLite (older than COT_PRUNE_DAYS).
    /// Call this periodically (e.g., once per day or on startup) to prevent unbounded SQLite growth.
    pub async fn prune_old_cot_entries(&self) {
        use chrono::Duration;
        let cutoff = (Utc::now() - Duration::days(COT_PRUNE_DAYS as i64)).to_rfc3339();
        match self.agent_memory.episode_store.prune_cot_entries(&cutoff) {
            Ok(deleted) => {
                if deleted > 0 {
                    println!(
                        "[COT] 🧹 Pruned {} COT entries older than {} days from SQLite",
                        deleted, COT_PRUNE_DAYS
                    );
                }
            }
            Err(e) => eprintln!("[COT] ⚠ Failed to prune old COT entries: {}", e),
        }
    }

    /// Hierarchical memory recall for "smarter" agents: combines 3 layers:
    ///   1. Knowledge Graph (relationship-based: symbol→regime→outcome paths)
    ///   2. Vector RAG (semantic similarity on recent trained episodes)
    /// Trained memory recall — 3-layer hierarchical search.
    /// Layer 1: Knowledge Graph (relationship-based recall)
    /// Layer 2: Vector RAG (semantic search for similar past episodes)
    /// Layer 3: Long-term agentmemory (cross-session trained lessons)
    pub async fn recall_trained_memory(
        &self,
        query_context: &str,
        top_k: usize,
    ) -> String {
        let mut parts = vec!["── HIERARCHICAL TRAINED MEMORY RECALL ──".to_string()];

        // Layer 1: Knowledge Graph (relationship-based recall)
        {
            let kg = self.agent_memory.knowledge_graph.read().await;
            if kg.is_built() {
                let graph_result = self.graph_recall_from_context(&kg, query_context);
                if graph_result.total_episodes > 0 {
                    parts.push(graph_result.summary);
                }
            }
        }

        // Layer 2: Local vector RAG (semantic search for similar episodes)
        {
            let vm = self.agent_memory.vector_memory.read().await;
            if !vm.is_empty() {
                match vm.search(query_context, top_k).await {
                    Ok(results) if !results.is_empty() => {
                        parts.push("LOCAL VECTOR (recent trained episodes):".to_string());
                        for r in results {
                            let regret = r
                                .regret_score
                                .map(|s| format!(" regret={:.2}", s))
                                .unwrap_or_default();
                            parts.push(format!(
                                "  - {} (sim {:.0}%{}): {}",
                                r.timestamp.format("%m/%d"),
                                r.similarity * 100.0,
                                regret,
                                r.summary_text
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        // Layer 3: Long-term agentmemory (cross-session trained intelligence)
        {
            let mem = cotrader_core::AgentMemoryClient::new();
            match mem
                .recall(&format!("trained lesson OR past action {}", query_context))
                .await
            {
                Ok(past) if !past.is_empty() => {
                    parts.push("LONG-TERM AGENTMEMORY (trained lessons across time):".to_string());
                    for p in past.iter().take(top_k) {
                        parts.push(format!("  - {}", p));
                    }
                }
                _ => {}
            }
        }

        if parts.len() == 1 {
            parts.push(
                "No strong trained memory match – proceeding with current rules + data only."
                    .to_string(),
            );
        }

        parts.join("\n")
    }

    /// Build the knowledge graph from closed episode store data.
    pub async fn rebuild_knowledge_graph(&self) {
        let episodes = match self.agent_memory.episode_store.fetch_closed_episodes_lite() {
            Ok(ep) => ep,
            Err(e) => {
                eprintln!("[GraphRAG] ⚠ Failed to fetch episodes for graph: {}", e);
                return;
            }
        };
        if episodes.is_empty() {
            return;
        }
        let mut kg = self.agent_memory.knowledge_graph.write().await;
        kg.build_from_episodes(&episodes);
    }

    /// Extract symbol/regime/direction from query context and run targeted graph traversal.
    fn graph_recall_from_context(
        &self,
        kg: &KnowledgeGraph,
        query_context: &str,
    ) -> cotrader_core::graph_rag::GraphRecallResult {
        let qc = query_context.to_uppercase();
        let symbol_nodes = kg.symbol_nodes();
        let found_symbol = symbol_nodes.iter().find(|s| qc.contains(s.as_str()));
        let known_regimes = ["TRENDINGBULL", "TRENDINGBEAR", "RANGING", "VOLATILE"];
        let found_regime = known_regimes.iter().find(|r| qc.contains(*r));

        match (found_symbol, found_regime) {
            (Some(sym), Some(reg)) => kg.query_symbol_regime(sym, reg),
            (Some(sym), None) => {
                let start = cotrader_core::graph_rag::GraphNode::Symbol(sym.to_string());
                kg.query_relationship(&start, None, 2)
            }
            (None, Some(reg)) => {
                let start = cotrader_core::graph_rag::GraphNode::Regime(reg.to_string());
                kg.query_relationship(&start, None, 2)
            }
            (None, None) => cotrader_core::graph_rag::GraphRecallResult {
                relationships: vec![],
                total_episodes: 0,
                aggregate_win_rate: 0.0,
                aggregate_avg_pnl: 0.0,
                summary: String::new(),
            },
        }
    }

    /// Start a new COT chain (root node) — creates an entry with chain_id = own id.
    pub async fn start_cot_chain(
        &self,
        agent: &str,
        input: &str,
        action: &str,
        reason: &str,
        confidence: f64,
    ) -> u64 {
        let id = self.agent_memory.cot_id_counter.fetch_add(1, Ordering::Relaxed);
        let entry = CotEntry {
            id,
            chain_id: id,
            parent_id: None,
            agent: agent.to_string(),
            input: input.to_string(),
            action: action.to_string(),
            reason: reason.to_string(),
            confidence,
            timestamp: Utc::now().to_rfc3339(),
            symbol: None,
        };
        let mut store = self.agent_memory.cot_store.write().await;
        store.push(entry);
        let store_len = store.len();
        if store_len > MAX_COT_RAM + COT_FLUSH_INTERVAL {
            let drain_count = store_len - MAX_COT_RAM;
            let overflow: Vec<_> = store.drain(0..drain_count).collect();
            let rows: Vec<crate::episode_store::CotLogRow> = overflow
                .iter()
                .map(|e| crate::episode_store::CotLogRow {
                    chain_id: e.chain_id,
                    agent: e.agent.clone(),
                    action: e.action.clone(),
                    reason: e.reason.clone(),
                    confidence: e.confidence,
                    symbol: e.symbol.clone(),
                    ts: e.timestamp.clone(),
                })
                .collect();
            let _ = self.agent_memory.episode_store.flush_cot_batch(&rows);
        }
        id
    }

    /// Push a summary COT entry that embeds multiple layer results as a single entry.
    /// This is the "summary mode" — instead of 17 per-agent COT entries per pipeline run,
    /// the pipeline emits ONE entry with all layer data embedded in the reason field as JSON.
    #[allow(clippy::too_many_arguments)]
    pub async fn push_summary_cot(
        &self,
        chain_id: u64,
        symbol: &str,
        layers: Vec<(&str, &str, f64, &str)>, // (layer_name, action, confidence, reason)
        final_action: &str,
        final_reason: &str,
    ) -> u64 {
        let summary_json = serde_json::json!({
            "type": "pipeline_summary",
            "layers": layers.iter().map(|(name, action, conf, reason)| serde_json::json!({
                "agent": name,
                "action": action,
                "confidence": conf,
                "reason": reason
            })).collect::<Vec<_>>(),
            "final_action": final_action,
            "final_reason": final_reason
        });
        self.push_cot(
            "PipelineSummary",
            &format!("Full pipeline for {}", symbol),
            final_action,
            &summary_json.to_string(),
            1.0,
            chain_id,
            Some(chain_id),
            Some(symbol.to_string()),
        )
        .await
    }

    /// Register external services (LLM, Kronos) with the ServiceManager
    /// and spawn the background health check loop with WS status broadcasts.
    pub async fn register_and_monitor_services(&self) {
        // Register LLM server
        let llm_endpoint = self.io.config.llm_endpoint.clone();
        let llm_name = format!("llm_{}", self.io.config.llm_provider);
        self.io.service_manager
            .register_service(&llm_name, &llm_endpoint)
            .await;

        // Register Kronos forecast server
        let kronos_endpoint = self.io.config.kronos_service_url.clone();
        self.io.service_manager
            .register_service("kronos", &kronos_endpoint)
            .await;

        // Register Broker API — determine endpoint from env vars
        let (broker_id, broker_endpoint) = detect_broker_endpoint();
        self.io.service_manager
            .register_service(&broker_id, &broker_endpoint)
            .await;

        // Clone the service manager and update_tx for the background loop
        let mgr = self.io.service_manager.clone();
        let tx = self.io.update_tx.clone();

        // Spawn background health check loop (every 30 seconds)
        tokio::spawn(async move {
            loop {
                // Run health checks
                mgr.run_all_health_checks().await;

                // Broadcast status via WebSocket
                let statuses = mgr.get_all_statuses().await;
                let msg = serde_json::json!({
                    "type": "service_status",
                    "services": statuses,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                })
                .to_string();
                let _ = tx.send(msg);

                // Wait 30 seconds
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        });

        // Print initial status
        self.io.service_manager.print_status_board().await;
    }

    /// Broadcast current service status via WebSocket.
    pub async fn broadcast_service_status(&self) {
        let statuses = self.io.service_manager.get_all_statuses().await;
        let msg = serde_json::json!({
            "type": "service_status",
            "services": statuses,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
        .to_string();
        let _ = self.io.update_tx.send(msg);
    }

    /// Build a JSON portfolio snapshot for HTTP status and WebSocket clients.
    pub fn portfolio_snapshot_json(portfolio: &crate::types::PortfolioState) -> serde_json::Value {
        let mr = portfolio.total_trades_today.max(1);
        serde_json::json!({
            "total_equity": portfolio.total_equity,
            "cash_balance": portfolio.cash_balance,
            "daily_pnl": portfolio.daily_pnl,
            "daily_pnl_pct": portfolio.daily_pnl_pct,
            "open_positions_count": portfolio.open_positions.len(),
            "open_positions": portfolio.open_positions,
            "total_trades_today": portfolio.total_trades_today,
            "trades_today": portfolio.total_trades_today,
            "winning_trades_today": portfolio.winning_trades_today,
            "losing_trades_today": portfolio.losing_trades_today,
            "consecutive_losses": portfolio.consecutive_losses,
            "win_rate": portfolio.winning_trades_today as f64 / mr as f64,
            "max_drawdown_today": portfolio.max_drawdown_today,
            "trading_enabled": portfolio.trading_enabled,
        })
    }

    /// Push a portfolio snapshot to all WebSocket subscribers.
    pub async fn broadcast_portfolio_snapshot(&self) {
        let portfolio = self.portfolio_store.portfolio.read().await;
        let mut snapshot = Self::portfolio_snapshot_json(&portfolio);
        if let Some(obj) = snapshot.as_object_mut() {
            obj.insert("type".to_string(), serde_json::json!("portfolio"));
        }
        let _ = self.io.update_tx.send(snapshot.to_string());
    }

    /// Add a step to an existing COT chain.
    /// Uses persist=false so per-agent pipeline entries are broadcast to the TUI
    /// for real-time display but are NOT stored in SQLite.
    /// Only summary entries (via push_summary_cot) persist to SQLite.
    ///
    /// NOTE: `quiet` flag — when true, the COT entry is skipped entirely.
    /// This eliminates per-agent write-lock contention on `cot_store` during
    /// automated pipeline runs (the summary entry at the end is still emitted).
    /// Use `quiet=false` for manual/interactive pipeline runs where TUI display matters.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_cot_step_quiet(
        &self,
        chain_id: u64,
        agent: &str,
        input: &str,
        action: &str,
        reason: &str,
        confidence: f64,
        symbol: Option<String>,
        quiet: bool,
    ) -> u64 {
        if quiet {
            // Skip entirely — no lock acquired, no WS broadcast.
            // The summary COT entry at the end of the pipeline still fires.
            return 0;
        }
        self.push_cot_with_persist(
            agent,
            input,
            action,
            reason,
            confidence,
            chain_id,
            Some(chain_id),
            symbol,
            false, // persist=false — real-time TUI display only, no SQLite
        )
        .await;
        self.agent_memory.cot_id_counter.load(Ordering::Relaxed) - 1
    }

    /// Legacy non-quiet wrapper — calls add_cot_step_quiet(quiet=false).
    #[allow(clippy::too_many_arguments)]
    pub async fn add_cot_step(
        &self,
        chain_id: u64,
        agent: &str,
        input: &str,
        action: &str,
        reason: &str,
        confidence: f64,
        symbol: Option<String>,
    ) -> u64 {
        self.add_cot_step_quiet(
            chain_id, agent, input, action, reason, confidence, symbol, false,
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Communication Log — transparent virtual communication layer.
    // ═══════════════════════════════════════════════════════════════════════

    /// Start a new pipeline run — creates a fresh CommunicationLog and returns the run ID.
    pub async fn start_pipeline_run(&self, symbol: &str) -> u64 {
        let run_id = self.io.pipeline_run_counter.fetch_add(1, Ordering::Relaxed);
        let log = CommunicationLog::new(run_id, symbol);
        *self.io.communication_log.write().await = log;
        run_id
    }

    /// Push an agent decision into the current pipeline run's communication log.
    /// Also broadcasts the decision via WebSocket for real-time TUI display.
    pub async fn push_agent_decision(&self, decision: AgentDecision) {
        // Broadcast to WebSocket for real-time TUI
        let update = serde_json::json!({
            "type": "agent_decision",
            "agent": decision.agent,
            "symbol": decision.symbol,
            "verdict": decision.verdict.label(),
            "reason": decision.verdict.reason(),
            "evidence": decision.evidence,
            "addressed_to": decision.addressed_to,
            "confidence": decision.verdict.confidence(),
            "timestamp": decision.timestamp,
        })
        .to_string();
        let _ = self.io.update_tx.send(update);

        // Store in communication log
        self.io.communication_log.write().await.push(decision);
    }

    /// Finalize the pipeline run — set the final verdict and summary.
    pub async fn finalize_pipeline_run(&self, final_verdict: &str, summary: &str) {
        let mut log = self.io.communication_log.write().await;
        log.final_verdict = final_verdict.to_string();
        log.summary = summary.to_string();

        // Broadcast final summary
        let update = serde_json::json!({
            "type": "pipeline_final",
            "run_id": log.run_id,
            "symbol": log.symbol,
            "final_verdict": final_verdict,
            "blocking_count": log.blocking_count(),
            "total_decisions": log.decisions.len(),
            "transcript": log.transcript(),
        })
        .to_string();
        let _ = self.io.update_tx.send(update);
    }

    /// Get a snapshot of the current communication log transcript (for debugging/display).
    pub async fn get_communication_transcript(&self) -> String {
        self.io.communication_log.read().await.transcript()
    }

    /// Get all blocking reasons from the current pipeline run.
    pub async fn get_blocking_reasons(&self) -> Vec<String> {
        self.io.communication_log.read().await.blocking_reasons()
    }

    /// Broadcast a transient live agent communication event directly to the WebSocket channel.
    /// Does not write to DB. Used for live Ollama and Kronos API call streams in the TUI.
    pub async fn push_live_comm(
        &self,
        from: &str,
        to: &str,
        action: &str,
        reason: &str,
        symbol: Option<String>,
    ) {
        let update = serde_json::json!({
            "type": "cot",
            "agent": from,
            "to": to,
            "action": action,
            "reason": reason,
            "confidence": 0.0,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "symbol": symbol
        })
        .to_string();
        let _ = self.io.update_tx.send(update);
    }
}

/// Detect which broker is configured and return its API endpoint.
/// Checks env vars for Alpaca and Zerodha credentials.
/// Returns (service_name, endpoint_url) — endpoint is empty if no external broker is configured.
fn detect_broker_endpoint() -> (String, String) {
    // Check CoTrader first, then Alpaca, then Zerodha
    let cotrader_url = std::env::var("COTRADER_BASE_URL").ok();
    let alpaca_key = std::env::var("ALPACA_API_KEY_ID").ok();
    let zerodha_key = std::env::var("ZERODHA_API_KEY").ok();

    if let Some(url) = cotrader_url {
        ("broker_cotrader".to_string(), url)
    } else if let Some(_key) = alpaca_key {
        let paper_mode = std::env::var("ALPACA_PAPER")
            .map(|v| v == "true")
            .unwrap_or(true);
        let endpoint = if paper_mode {
            "https://paper-api.alpaca.markets".to_string()
        } else {
            "https://api.alpaca.markets".to_string()
        };
        ("broker_alpaca".to_string(), endpoint)
    } else if std::env::var("COTRADER_BASE_URL").is_ok() {
        let endpoint = std::env::var("COTRADER_BASE_URL").unwrap();
        ("broker_cotrader".to_string(), endpoint)
    } else if let Some(_key) = zerodha_key {
        (
            "broker_zerodha".to_string(),
            "https://api.kite.trade".to_string(),
        )
    } else {
        // No live broker configured — register with empty endpoint
        // The ServiceManager will treat it as always healthy (no external ping needed).
        ("broker_paper".to_string(), String::new())
    }
}

pub async fn initialize_autonomous_system(
    paper_broker: Arc<dyn cotrader_core::paper_engine::BrokerAdapter>,
) -> Result<crate::AutonomousOrchestrator, Box<dyn std::error::Error + Send + Sync>> {
    let memory = MemoryStore::new("rat.redb")?;
    let rules = DisciplineRules::default();
    let config = Config::default();

    let state = SharedState::with_event_bus(
        memory,
        rules,
        config,
        "rat_history.db",
        None,
        paper_broker,
    )
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    // Load live economic calendar (falls back to built-in events)
    state.refresh_calendar().await;

    // Register external services with the ServiceManager
    state.register_and_monitor_services().await;

    // Build knowledge graph from closed episodes immediately (so recall has graph data from start)
    state.rebuild_knowledge_graph().await;

    Ok(crate::AutonomousOrchestrator::new(state))
}
