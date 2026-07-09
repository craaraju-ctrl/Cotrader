# CoTrader — System Architecture Specification

> Version: 2.0  
> Last Updated: 2026-07-10  
> Classification: Internal Engineering Specification

---

## 1. System Overview

CoTrader is a production-grade autonomous trading system built in Rust, featuring a 4-layer parallel validation pipeline, Cornish-Fisher VaR risk management, FinBERT sentiment analysis, and ML-driven signal arbitration.

### 1.1 Design Principles

| Principle | Implementation |
|-----------|----------------|
| **Determinism** | All risk rules are deterministic; ML provides signals, not decisions |
| **Isolation** | Each validation layer runs independently; failures don't cascade |
| **Auditability** | Every decision logged with full reasoning chain |
| **Adaptability** | Trust weights evolve based on trade outcomes |
| **Cross-platform** | All paths resolve dynamically; no OS-specific hardcoding |

---

## 2. Crate Architecture

```
cotrader/
├── crates/
│   ├── cotrader-core/           Foundation library
│   ├── cotrader-autonomous/     4-layer pipeline, agents, rules engine
│   ├── cotrader-ml/             ML models (Chronos, Llama, regime, patterns)
│   ├── cotrader-orchestrator/   Service orchestration, HTTP/WebSocket
│   ├── cotrader-runtime/        CLI and daemon management
│   ├── cotrader-eventbus/       Inter-agent event system
│   ├── cotrader-tui/            Terminal UI (Ratatui)
│   └── cotrader-broker-cotrader/ Broker adapter
├── memory/                     Agentic memory server (port 3111)
└── storage/                    Centralized database storage
```

### 2.1 Crate Dependencies

```
cotrader-core (foundation)
    ↑
cotrader-ml (ML models)
    ↑
cotrader-autonomous (pipeline + agents)
    ↑
cotrader-orchestrator (HTTP server + loops)
    ↑
cotrader-runtime (CLI binary)
```

---

## 3. Core Module (`cotrader-core`)

### 3.1 Key Modules

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `config.rs` | Configuration management | `Config`, `StorageConfig`, `SystemConfig`, `LlamaBackend` |
| `risk.rs` ★ | Cornish-Fisher VaR computation | `VaRConfig`, `VaRResult` |
| `sentiment.rs` ★ | FinBERT sentiment extraction | `SentimentConfig`, `SentimentResult` |
| `disciplined_core.rs` | Pivot points, confluence scoring | `PivotLevels`, `MarketContext` |
| `patterns.rs` | 27+ candlestick pattern detection | `CandlestickPattern` |
| `advanced_patterns.rs` | 17 chart patterns | `AdvancedPattern` |
| `paper_engine.rs` | Paper trading simulation | `BrokerAdapter`, `TradeSetup` |
| `portfolio_analytics.rs` | Kelly criterion, efficient frontier | `KellyAllocation` |

### 3.2 Storage Configuration

```rust
pub struct StorageConfig {
    pub base_dir: PathBuf,  // Default: <workspace>/storage/
}

impl StorageConfig {
    pub fn main_db(&self) -> PathBuf { self.base_dir.join("cotrader.db") }
    pub fn orders_db(&self) -> PathBuf { self.base_dir.join("orders.db") }
    pub fn memory_db(&self) -> PathBuf { self.base_dir.join("memory.db") }
    pub fn model_dir(&self) -> PathBuf { self.base_dir.join("models") }
}
```

---

## 4. Autonomous Module (`cotrader-autonomous`)

### 4.1 4-Layer Parallel Validation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    4-LAYER PARALLEL VALIDATION                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Layer 1: RULES (35%)              Layer 2: ML/SIGNAL (25%)                │
│  ├─ 22 Hard Rules                  ├─ Confluence scoring                   │
│  ├─ Pivot points                   ├─ Technical indicators                 │
│  ├─ Regime-adaptive thresholds     └─ Deterministic signal                 │
│  └─ VaR Emergency Gate ★                                                   │
│                                                                             │
│  Layer 3: CHRONOS (25%)            Layer 4: SENTIMENT (15%) ★              │
│  ├─ T5-based forecasting           ├─ FinBERT (BGE-small-en-v1.5)         │
│  ├─ 2048 context window            ├─ 384-dim embeddings                   │
│  └─ 64-step predictions            └─ Directional modifier [-1, +1]        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Key Modules

| Module | Purpose |
|--------|---------|
| `tri_level_validator.rs` | 4-layer parallel validation pipeline |
| `hard_rules_gate.rs` | 22 deterministic risk rules |
| `risk_guardian.rs` | Position sizing, leverage limits |
| `episode_store.rs` | Trade history and outcome tracking |
| `event_driven_pipeline.rs` | Event-triggered analysis |
| `orchestrator_struct.rs` | Main orchestrator logic |

### 4.3 Trust Weight Learning

After each trade close, trust weights are updated:

```rust
// Correct layers: weight *= 1.0 + lr * accuracy (lr=0.05)
// Incorrect layers: weight *= 1.0 - lr * regret
// Clamped to [0.10, 0.60], normalized to sum to 1.0
```

---

## 5. ML Module (`cotrader-ml`)

### 5.1 Model Registry

| Model | Architecture | Input | Output | Framework |
|-------|--------------|-------|--------|-----------|
| Chronos-Bolt | T5 encoder-decoder | 2048 timesteps | 64-step forecast | Candle |
| Regime Classifier | MLP (30→16→5) | 30 indicators | 5 regimes | Candle |
| Signal Scorer | MLP (34→16→1) | 34 features | P(profitable) | Candle |
| Win Probability | Logistic (48→1) | 48 features | P(win) | Linfa |
| Pattern Detector | CNN (20×5→4) | 20-bar OHLCV | 4 directions | Candle |
| Strategy Selector | RandomForest | 48 features | Best strategy | Linfa |
| Reasoning Engine | Llama-3.2-3B (Q4_K_M) | ArbitrationInput | FinalSignal | Candle |

### 5.2 Feature Flags

```toml
[features]
default = ["ml"]
ml = ["dep:candle-core", "dep:candle-nn", "dep:candle-transformers",
      "dep:hf-hub", "dep:linfa", "dep:linfa-trees", "dep:tokenizers"]
```

All ML dependencies are optional; system falls back to threshold-based logic when no models are trained.

---

## 6. Event Bus (`cotrader-eventbus`)

### 6.1 Event Types

```rust
pub enum RatEvent {
    Signal { symbol, direction, confidence },
    MarketPrice { symbol, price, timestamp },
    PortfolioSnapshot { equity, positions },
    Health { status, uptime },
    SystemControl { command },
}
```

### 6.2 Communication Pattern

```
Publisher → EventBus (tokio::broadcast) → Subscriber
```

---

## 7. Memory Server (`memory/`)

### 7.1 Architecture

- HTTP server on port 3111
- SQLite backend for persistence
- Vector embeddings via fastembed
- Knowledge graph via petgraph

### 7.2 API Surface

| Endpoint | Purpose |
|----------|---------|
| `POST /observations` | Store new observations |
| `GET /search` | Semantic search |
| `POST /consolidate` | Merge duplicate memories |
| `GET /graph` | Knowledge graph query |

---

## 8. Cross-Platform Path Resolution

### 8.1 Dynamic Path Strategy

All paths resolve dynamically:

```rust
impl SystemConfig {
    pub fn rat_dir() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".rat")
    }
}
```

### 8.2 Database Paths

| Database | Location | Purpose |
|----------|----------|---------|
| `cotrader.db` | `storage/` | Main trading database |
| `orders.db` | `storage/` | Live order tracking |
| `memory.db` | `storage/` | Agentic memory |

### 8.3 Supported Platforms

| Platform | Architecture | Status |
|----------|--------------|--------|
| macOS | Apple Silicon (M1/M2/M3) | ✅ Fully supported |
| macOS | Intel x86_64 | ✅ Fully supported |
| Linux | Ubuntu x86_64 | ✅ Fully supported |
| Linux | ARM64 | ✅ Fully supported |
| Windows | WSL2 | ⚠️ Experimental |

---

## 9. Security Considerations

### 9.1 Secrets Management

- API keys stored in environment variables
- No hardcoded secrets in source code
- `.gitignore` blocks `.env` files

### 9.2 Database Security

- WAL mode for concurrent access
- No network-accessible database ports
- Local-only file permissions

---

## 10. Performance Characteristics

| Metric | Target | Notes |
|--------|--------|-------|
| Pipeline latency | < 500ms | 4 layers in parallel |
| LLM inference | < 6s | Only on conflict |
| VaR computation | < 10ms | Rolling 60-bar window |
| Sentiment analysis | < 100ms | Keyword + embedding |
| Memory usage | < 4GB | With all models loaded |

---

*End of Architecture Specification*
