# RAT Agent — Architecture & Technical Documentation

## 1. Built vs Pending Components

### Built Components (Complete)

| Component | Crate | Files | Status |
|-----------|-------|-------|--------|
| Market Connector | `rat-brokers/src/live/` | 3 | ✅ Binance REST API |
| Intelligence Core | `rat-indicators/` | 98 | ✅ 16 indicators |
| Agent System | `rat-agents/` | 128 | ✅ 21 agents |
| Storage Layer | `rat-core/src/memory.rs` | 32 | ✅ SQLite + vectors |
| Risk Engine | `rat-rules/` | 36 | ✅ 29 rules |
| Pipeline Runner | `rat-pipeline/src/runner/` | 53 | ✅ 5-phase workflow |
| TUI Dashboard | `rat-tui/` | 39 | ✅ Terminal UI |
| Backtesting | `rat-pipeline/src/backtest/` | 5 | ✅ Engine + simulator |
| Monitoring | `rat-pipeline/src/monitoring/` | 6 | ✅ Health + metrics |
| Alerts | `rat-pipeline/src/alerts/` | 6 | ✅ Telegram + email |

### Pending Components

| Component | Priority | Description |
|-----------|----------|-------------|
| Live broker orders | P1 | Real Binance/Zerodha order placement |
| Walk-forward optimization | P2 | Rolling window strategy testing |
| Multi-exchange arbitrage | P2 | Cross-exchange opportunities |
| ML pattern recognition | P3 | CNN/LSTM for pattern detection |
| Reinforcement learning | P3 | PPO/SAC strategy optimization |

---

## 2. Rust Libraries & Frameworks

| Section | Library | Version | Purpose |
|---------|---------|---------|---------|
| **Runtime** | Tokio | 1.52 | Async execution, channels, timers |
| **HTTP Server** | Axum | 0.8 | REST API for TUI |
| **HTTP Client** | reqwest | 0.13 | Binance API, market data |
| **WebSocket** | tokio-tungstenite | 0.24 | Real-time price streaming |
| **TUI** | ratatui | 0.30 | Terminal dashboard |
| **Memory** | agentic-memory | custom | SQLite + vector search |
| **Database** | rusqlite | 0.32 | Order/episode storage |
| **Serialization** | serde | 1.0 | JSON handling |
| **Time** | chrono | 0.4 | Timestamps, scheduling |
| **Events** | tokio broadcast | 1.52 | Inter-agent messaging |

---

## 3. Tokio Broadcast Channel Architecture

```
PipelineRunner
    │
    └── broadcast::channel(1024) → PipelineEvent
            │
            ├── MarketData { symbol, price, timestamp }
            ├── SignalGenerated { symbol, action, confidence }
            ├── RiskChecked { symbol, passed, reason }
            ├── TradeExecuted { symbol, action, size, price }
            ├── TradeOutcome { symbol, pnl, lesson }
            ├── AgentDecision { agent, decision, timestamp }
            └── Error { source, message }

Each agent: rx = event_tx.subscribe()
Pattern: One producer, multiple consumers, non-blocking
```

---

## 4. AI/ML Integration Plan

**Current:** All decisions deterministic via rules and indicators.

**Future ML integration:**

| Component | Algorithm | Purpose |
|-----------|-----------|---------|
| Pattern Recognition | CNN/LSTM | OHLCV pattern detection |
| Sentiment Analysis | NLP Transformer | News/social scoring |
| Regime Detection | Hidden Markov Model | Market state classification |
| Adaptive Kelly | Online Learning | Optimal position sizing |
| Strategy Optimization | PPO/SAC | Reinforcement learning |

**Approach:** Optional features behind feature flags, deterministic fallbacks always available.

---

## 5. Risk Management Precision

| Rule | Type | Threshold |
|------|------|-----------|
| Daily drawdown | Hard stop | 2% |
| Max position size | Per-trade | 5% equity |
| Portfolio heat | Aggregate | 30% max |
| Black swan detector | Flash crash | >5% move |
| Loss circuit breaker | Behavioral | 3 consecutive |
| Volatility stops | ATR-based | Dynamic |
| Correlation limits | Concentration | Max 5 correlated |

**Priority system:** Critical > High > Medium > Low (29 rules total)

---

## 6. Real-Time Data Processing

**Yes, fully real-time:**

```
Binance WebSocket → Price Feed → RSI/MACD → Signal → Risk → Execute
       ↓                ↓            ↓          ↓       ↓        ↓
   100ms latency   Real prices  50-bar    Weighted  29     Paper
                   BTC $58K     history   scores   rules  trades
```

**Data flow:**
1. WebSocket connects to `wss://stream.binance.com`
2. Prices arrive every ~100ms
3. RSI (14-period) + MACD (12,26,9) on 50-bar history
4. Combined score → BUY/SELL/HOLD with confidence
5. 29 risk rules validate
6. Execute via PaperBroker or live broker

**Fallback:** REST polling every 5 seconds if WebSocket fails.

---

## Quick Start

```bash
# Build
cargo build --release

# Run pipeline (paper mode)
./target/release/rat-pipeline --symbols BTC,ETH --mode paper

# Run TUI
./target/release/rat-tui

# Run tests
cargo test --workspace
```

---

## File Count

| Category | Count |
|----------|-------|
| Total crates | 28 |
| Total .rs files | 700+ |
| Agents | 128 files |
| Indicators | 98 files |
| Rules | 36 files |
| Tests | 112 passing |
