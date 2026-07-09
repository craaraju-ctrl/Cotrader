pub mod advanced_patterns;
pub mod agent;
pub mod asset_class;
pub mod backtest;
pub mod market_hours;
pub mod risk_engine;
pub mod risk; // Cornish-Fisher VaR computation
pub mod sentiment; // FinBERT sentiment extraction
pub mod binance;
pub mod breaker_net; // Global circuit-breaker coordination (fault-isolation audit D4)
pub mod broker;
pub mod cache; // Multi-tier trading cache with predictive eviction
pub mod tredo_client; // Tredo Exchange REST client — single data gateway
pub mod calendar;
pub mod config;
pub mod disciplined_core;
pub mod episode;
pub mod goals;
pub mod live_calendar;
pub mod memory;
pub mod messages;
pub mod news;
pub mod notifier;
pub mod options;
pub mod paper_engine;
pub mod patterns;
pub mod portfolio_analytics;
pub mod role;
pub mod service_manager;
pub mod skill_aggregator; // Weighted ensemble aggregation for structured SkillResult outputs
pub mod skills; // AgentSkill trait for building skills/tools (pluggable agent capabilities)
pub mod memory_integration;
pub mod market_regime;
pub mod symbol;

pub use advanced_patterns::{
    detect_advanced_patterns, detect_channel, detect_double_bottom, detect_double_top,
    detect_falling_wedge, detect_flag, detect_head_and_shoulders, detect_pennant,
    detect_rising_wedge, format_advanced_patterns, AdvancedPattern, AdvancedPatternType,
    ChannelPattern, DoubleTopBottomPattern, FlagPennantPattern, HeadShouldersPattern, WedgePattern,
};
pub use agent::{Agent, AgentInput, AgentOutput, AgentTier, SkillDirection};
pub use agentmemory::AgentMemoryClient;
pub use backtest::{BacktestResult, Backtester, TradeDirection, TradeSetup};
pub use binance::{
    is_crypto_symbol, normalize_base_symbol, pair_candidates,
    to_binance_pair, yahoo_symbol,
};
pub use cache::{ActiveOrder, CachePriority, CacheStats, OrderSide, OrderStatus, SpotPrice, TradingCache};
pub use tredo_client::{
    fetch_tredo_price, try_fetch_tredo_price, fetch_tredo_candles, try_fetch_tredo_candles,
    fetch_tredo_ticker_24hr, fetch_tredo_orderbook, fetch_tredo_news,
    tredo_base_url, to_tredo_symbol, from_tredo_symbol,
};
pub use calendar::{generate_economic_calendar, CalendarEvent, EventImpact};
pub use config::{Config, StorageConfig};
pub use disciplined_core::{
    apply_trained_memory_to_rules, calculate_confluence_score, calculate_pivot_points,
    check_risk_limits, get_discipline_summary, is_in_trading_session, validate_trade_setup,
    DisciplineCheck, DisciplineRules, MarketContext, PivotLevels, PivotMethod, SkillVote,
    TrendDirection,
};
pub use episode::{
    MarketStateSnapshot, ReasoningStep, TradeOutcome, TradingEpisode,
};
pub use goals::{TradingGoals, TradingMode};
pub use live_calendar::{fetch_economic_calendar_live, CalendarSource};

/// OHLCV bar type — used by patterns and advanced patterns.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OhlcvBar {
    pub timestamp: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}
pub use memory::MemoryStore;
pub use messages::{AgentMessage, LLMRequest, LLMResponse};
pub use news::{NewsContext, NewsFetcher, NewsItem};
pub use options::{
    analyze_options_chain, bear_put_spread, black_scholes_greeks, black_scholes_price,
    bull_call_spread, covered_call, futures_fair_value, implied_volatility, iron_condor,
    long_straddle, long_strangle, protective_put, ExerciseStyle, FuturesContract, OptionContract,
    OptionSide, OptionsChain, OptionsSignal, OptionsStrategy,
};
pub use paper_engine::*;
pub use patterns::{
    detect_patterns, detect_patterns_multi_tf, format_mtf_confirmation, format_patterns,
    CandlestickPattern, ConfirmationLevel, MultiTfPatternConfirmation,
};
pub use portfolio_analytics::{
    efficient_frontier_points, kelly_criterion_fraction, mean_variance_optimize,
    optimal_kelly_portfolio, KellyAllocation, PortfolioVar,
};
pub use role::AgentRole;
pub use service_manager::{ConnectionStatus, ServiceManager, ServiceStatus};
pub use skill_aggregator::{AggregatedSignal, SkillAggregator};
pub use memory_integration::{MemoryIntegration, PolicyEntry};
pub use market_regime::MarketRegime;
pub use risk::{compute_cornish_fisher_var, compute_var_from_returns, check_var_emergency_gate, VaRConfig, VaRResult};
pub use sentiment::{extract_sentiment, extract_sentiment_from_text, init_embedding_model, SentimentConfig, SentimentResult};
pub use symbol::{bare_symbol_set, symbols_match, SymbolPair};
pub mod agentmemory;
