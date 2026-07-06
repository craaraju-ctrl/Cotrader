# CoTrader — Autonomous Trading System

[![Rust](https://img.shields.io/badge/rust-1.96%2B-blue)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

A production-grade autonomous trading system with **8 core agents**, **ML predictions**, **neurosymbolic verification**, and **NLP sentiment analysis** — all in pure Rust.

## Architecture

```
                    ┌─────────────────────────────────────────────┐
                    │              RAT ORCHESTRATOR                │
                    └─────────────────────────────────────────────┘
                                      │
        ┌─────────────────────────────┼─────────────────────────────┐
        ▼                             ▼                             ▼
┌───────────────┐           ┌───────────────┐           ┌───────────────┐
│   ML MODELS   │           │ NEUROSYMBOLIC │           │   NLP ENGINE  │
│   (cotrader-ml)    │           │ (rat-neuro)   │           │   (cotrader-nlp)   │
│ • Regime      │           │ • 10 rules    │           │ • Sentiment   │
│ • Signal      │           │ • Rule learn  │           │ • NER         │
│ • Win Prob    │           │ • Kronos fuse │           │ • Events      │
│ • Patterns    │           │               │           │ • Summarize   │
│ • Strategy    │           │               │           │               │
└───────────────┘           └───────────────┘           └───────────────┘
        │                             │                             │
        └─────────────────────────────┼─────────────────────────────┘
                                      ▼
                    ┌─────────────────────────────────────────────┐
                    │           8 CORE AGENTS                     │
                    ├─────────────────────────────────────────────┤
                    │ Analysis → Planning → Decision → Execution  │
                    │    ↓          ↓          ↓          ↓       │
                    │ Observation ← Risk ← Psychology ← Evolution │
                    └─────────────────────────────────────────────┘
                                      │
                    ┌─────────────────┼─────────────────┐
                    ▼                 ▼                 ▼
              ┌──────────┐     ┌──────────┐     ┌──────────┐
              │ Episode  │     │ Vector   │     │ Knowledge│
              │ Store    │     │ Memory   │     │ Graph    │
              │ (SQLite) │     │ (JSON)   │     │(petgraph)│
              └──────────┘     └──────────┘     └──────────┘
```

## 8 Core Agents

| Agent | Purpose | Key Features |
|-------|---------|--------------|
| **Analysis** | Market data processing | 26+ indicators, ML regime detection, NLP sentiment |
| **Planning** | Strategy & signals | Strategy selection, Kelly sizing, trade setup |
| **Decision** | Cross-validation | Neurosymbolic verification, ML scoring, conviction |
| **Implementation** | Order execution | Paper/live trading, position monitoring, SL/TP |
| **Observation** | Outcome tracking | Trade logging, rule learning, performance metrics |
| **Risk** | Risk management | 7 risk checks, position adjustments, overtrading prevention |
| **Psychology** | Behavioral analysis | 5 bias types, emotional state detection, discipline |
| **Evolution** | Self-improvement | ML training, weight tuning, model management |

## Three Intelligence Layers

```
Neural (ML)          → Pattern recognition, predictions (cotrader-ml)
Symbolic (rules)     → Formal logic, constraints (cotrader-neurosymbolic)
Neurosymbolic        → LLM + rules combined verification
NLP                  → Text understanding, sentiment, events (cotrader-nlp)
```

## Quick Start

```bash
# Clone and build
git clone https://github.com/varma/cotrader.git
cd cotrader
cargo build --release

# Demo the 8-agent pipeline
cargo run --bin rat-cli demo BTC 58500.0

# Start the full system
./start.sh

# Monitor with TUI
./target/release/rat-tui

# Run tests
cargo test --workspace
```

## Project Structure (35 crates)

```
cotrader/
├── crates/
│   ├── rat-core/           Core types, memory, paper engine
│   ├── cotrader-autonomous/     8 agents, pipeline, all logic
│   ├── cotrader-ml/             5 ML models (regime, signal, win prob, patterns, strategy)
│   ├── cotrader-neurosymbolic/  Neurosymbolic verification (10 rules, rule learning)
│   ├── cotrader-nlp/            NLP engine (sentiment, NER, events, summarization)
│   ├── rat-orchestrator/   Service orchestration
│   ├── rat-runtime/        CLI and daemon management
│   ├── rat-eventbus/       Inter-agent event system
│   ├── rat-indicators/     16 technical indicators
│   ├── rat-patterns/       Candlestick + chart patterns
│   ├── rat-regime/         Market regime detection
│   ├── rat-strategies/     10 trading strategies
│   ├── rat-reasoning/      7 reasoning chains
│   ├── rat-rules/          29 risk rules
│   ├── rat-risk/           Position sizing, drawdown
│   ├── rat-skills/         Pluggable agent skills
│   ├── rat-sentiment/      News sentiment analysis
│   ├── rat-feeds/          Market data feeds
│   ├── rat-broker-*/       7 broker adapters
│   ├── rat-memory/         Agentic memory server
│   ├── rat-memory-client/  Memory client SDK
│   ├── rat-market-data/    Market data service
│   ├── rat-exchange-client/ Exchange API client
│   ├── rat-tui/            Terminal UI
│   ├── rat-server/         HTTP/WebSocket server
│   ├── rat-metrics/        Prometheus metrics
│   ├── rat-watchdog/       Hardware kill switch
│   ├── rat-compliance/     Compliance checks
│   └── rat-ecosystem/      Ecosystem tools
├── memory/                 Agentic memory (Rust)
├── kronos/                 Time-series forecasting
├── data/                   Model storage
├── scripts/                Build/deploy scripts
├── start.sh                System launcher
├── stop.sh                 System shutdown
└── build.sh                Build automation
```

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 2021 |
| Async | Tokio |
| Web | Axum |
| Database | SQLite (rusqlite), redb, LanceDB |
| ML | Candle 0.8, Linfa 0.7 |
| NLP | Ollama (llama3.2:3b) |
| Memory | Agentic Memory (port 3111) |
| Forecasting | Kronos (port 8000) |
| UI | Ratatui TUI |

## ML Models

| Model | Type | Input | Output |
|-------|------|-------|--------|
| Regime Classifier | MLP | 30 indicators | 5 regimes |
| Signal Scorer | MLP | 34 features | P(profitable) |
| Win Probability | Logistic | 48 features | P(win) for Kelly |
| Pattern Detector | CNN | 20-bar OHLCV | 4 directions |
| Strategy Selector | Weighted | 48 features | Best strategy |

## Neurosymbolic Rules

10 formal trading rules + learned rules from outcomes:

| Rule | Condition | Action |
|------|-----------|--------|
| R001 | Portfolio heat > 10% | BLOCK |
| R002 | 5+ consecutive losses | BLOCK |
| R003 | Daily drawdown > 5% | BLOCK |
| R010 | Confidence < 35% | BLOCK |
| R011 | R:R < 2.0 | REDUCE |
| R012 | ML disagrees + BUY | BLOCK |
| R020 | Volatile regime | REDUCE |
| R021 | Low liquidity | REDUCE |
| R022 | 10+ trades today | WARN |
| R030 | Volume > 3x | WARN |

## Configuration

```bash
# Environment variables
OLLAMA_BASE_URL=http://localhost:11434  # LLM endpoint
KRONOS_URL=http://localhost:8000        # Forecast service
WATCHLIST=BTC,ETH,SOL,BNB              # Default symbols
```

## License

MIT
