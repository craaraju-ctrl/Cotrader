# CoTrader — Autonomous Trading System

[![Rust](https://img.shields.io/badge/rust-1.96%2B-blue)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

A production-grade autonomous trading system with **4-layer parallel validation**, **Cornish-Fisher VaR risk management**, **FinBERT sentiment analysis**, and **ML-driven signal arbitration** — all in pure Rust.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        4-LAYER VALIDATION PIPELINE                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Layer 1: RULES (35%)              Layer 2: ML/SIGNAL (25%)                │
│  ├─ 22 Hard Rules                  ├─ Confluence scoring                   │
│  ├─ Pivot points (Classic/Woodie)  ├─ Technical indicators                 │
│  ├─ Regime-adaptive thresholds     └─ Deterministic signal                 │
│  └─ VaR Emergency Gate ★                                                   │
│                                                                             │
│  Layer 3: CHRONOS (25%)            Layer 4: SENTIMENT (15%) ★ NEW          │
│  ├─ T5-based forecasting           ├─ FinBERT (BGE-small-en-v1.5)         │
│  ├─ 2048 context window            ├─ 768-dim embeddings                   │
│  └─ 64-step predictions            └─ Directional modifier [-1, +1]        │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                    2-of-4 Agreement Gate → LLM Arbitration                  │
│                         (Llama-3.2-3B via Candle/Ollama)                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 4-Layer Parallel Validation

| Layer | Weight | Source | Purpose |
|-------|--------|--------|---------|
| **Rules** | 35% | `hard_rules_gate.rs` | 22 deterministic risk rules + pivot/confluence signal |
| **ML/Signal** | 25% | `check_llm_layer()` | Confluence + trend analysis for deterministic signal |
| **Chronos** | 25% | `chronos_bolt.rs` | T5-based time series forecasting (64-step horizon) |
| **Sentiment** | 15% | `sentiment.rs` ★ | FinBERT news sentiment with directional modifier |

## Cornish-Fisher VaR Emergency Gate ★ NEW

The VaR emergency gate provides dynamic statistical drawdown boundaries that replace static risk checks.

### Formula

```
Z_cf = Z_α + (Z_α² - 1) × S/6 + (Z_α³ - 3Z_α) × K/24 - (2Z_α³ - 5Z_α) × S²/36

Where:
  Z_α = norm.ppf(0.01) = -2.326 (99% confidence)
  S   = rolling skewness of returns
  K   = rolling excess kurtosis of returns

VaR  = -(μ + Z_cf × σ)
```

### Emergency Gate Logic

```
IF VaR_alpha > risk_tolerance (5%)
   OR volatility_ratio > volatility_cap (3x)
THEN:
   Force ALL layer signals to "HOLD"
   Override any bullish signals
   Log: "[VaR] ⚠ EMERGENCY TRIGGERED"
```

### Configuration

```rust
pub struct VaRConfig {
    pub confidence_level: f64,      // 0.99 (99% VaR)
    pub lookback_window: usize,     // 60 bars (rolling window)
    pub risk_tolerance: f64,        // 0.05 (5% max portfolio VaR)
    pub volatility_cap: f64,        // 3.0 (max sigma multiplier)
    pub enabled: bool,              // default: true
}
```

## FinBERT Sentiment Pipeline ★ NEW

### Architecture

```
News Headlines → Keyword Classification → Embedding (BGE-small-en-v1.5)
                                              ↓
                                   384-dim Vector → Sentiment Score
                                              ↓
                                   [-1.0, +1.0] Directional Modifier
```

### Sentiment Score Mapping

| Score Range | Label | Action |
|-------------|-------|--------|
| < -0.6 | hyper-bearish | Strong SELL signal |
| -0.6 to -0.2 | bearish | SELL signal |
| -0.2 to +0.2 | neutral | HOLD |
| +0.2 to +0.6 | bullish | BUY signal |
| > +0.6 | hyper-bullish | Strong BUY signal |

### LLM Prompt Integration

```
Sentiment:   score=+0.450 conf=0.72 (bullish)
```

## 8 Core Agents

| Agent | Purpose | Key Features |
|-------|---------|--------------|
| **Analysis** | Market data processing | 26+ indicators, ML regime detection |
| **Planning** | Strategy & signals | Strategy selection, Kelly sizing |
| **Decision** | Cross-validation | ML scoring, conviction analysis |
| **Implementation** | Order execution | Paper/live trading, SL/TP monitoring |
| **Observation** | Outcome tracking | Trade logging, rule learning |
| **Risk** | Risk management | VaR gate, position adjustments |
| **Psychology** | Behavioral analysis | 5 bias types, discipline enforcement |
| **Evolution** | Self-improvement | ML training, weight tuning |

## Quick Start

```bash
# Clone and build
git clone https://github.com/varma/cotrader.git
cd cotrader
cargo build --release

# Demo the 4-layer pipeline
cargo run --bin cotrader-cli demo BTC 58500.0

# Start the full system
./start.sh

# Monitor with TUI
./target/release/cotrader-tui

# Run tests
cargo test --workspace
```

## Project Structure

```
cotrader/
├── crates/
│   ├── cotrader-core/           Core types, risk (VaR), sentiment, memory
│   ├── cotrader-autonomous/     4-layer pipeline, agents, rules engine
│   ├── cotrader-ml/             ML models (Chronos, Llama, regime, patterns)
│   ├── cotrader-orchestrator/   Service orchestration
│   ├── cotrader-runtime/        CLI and daemon management
│   ├── cotrader-eventbus/       Inter-agent event system
│   ├── cotrader-tui/            Terminal UI (Ratatui)
│   └── cotrader-broker-cotrader/ Broker adapter
├── memory/                 Agentic memory server (port 3111)
├── data/                   Model storage, logs
├── scripts/                Build/deploy scripts
├── start.sh                System launcher
├── stop.sh                 System shutdown
└── build.sh                Build automation
```

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 2021 |
| Async Runtime | Tokio |
| ML Framework | Candle 0.8 (neural networks) |
| Classical ML | Linfa 0.7 (GBT, RandomForest) |
| Time Series | Chronos-Bolt (T5-based) |
| LLM | Llama-3.2-3B (Candle GGUF) |
| Embeddings | fastembed (BGE-small-en-v1.5) |
| Database | SQLite (rusqlite), redb |
| UI | Ratatui TUI |

## ML Models

| Model | Type | Input | Output |
|-------|------|-------|--------|
| Chronos-Bolt | T5 encoder-decoder | 2048 timesteps | 64-step forecast |
| Regime Classifier | MLP | 30 indicators | 5 regimes |
| Signal Scorer | MLP | 34 features | P(profitable) |
| Win Probability | Logistic | 48 features | P(win) for Kelly |
| Pattern Detector | CNN | 20-bar OHLCV | 4 directions |
| Strategy Selector | RandomForest | 48 features | Best strategy |

## Hard Rules (22 Rules)

| Priority | Rule | Threshold | Action |
|----------|------|-----------|--------|
| Critical | Trading enabled | Must be true | BLOCK |
| Critical | Daily drawdown | ≤ 2% | BLOCK |
| Critical | Red folder day | No high-impact events | BLOCK |
| High | Portfolio heat | ≤ 10% (vol-adjusted) | BLOCK |
| High | Consecutive losses | < 4 | BLOCK |
| High | Daily trades | < 10 | BLOCK |
| High | Kelly sizing | ≤ 2x half-Kelly | BLOCK |
| Medium | Regime safety | No BUY in bear+low confluence | BLOCK |
| Medium | Confluence min | Regime-adaptive (35-80%) | BLOCK |
| Low | Max positions | < 3 per symbol | WARN |

## Configuration

```bash
# Environment variables
PAPER_MODE=true                    # Paper trading mode
RAT_MAX_DAILY_TRADES=10            # Max trades per day

# System config (from ~/.rat/system.toml)
[llama_backend]
type = "Ollama"
url = "http://localhost:11434"
model = "llama3.2:3b"
```

## Cross-Platform Support

- **macOS**: Apple Silicon (M1/M2/M3) and Intel
- **Linux**: Ubuntu x86_64/ARM64
- **Windows**: Experimental support via WSL2

All paths resolve dynamically relative to `~/.rat/` or the runtime directory. No hardcoded macOS-specific paths.

## License

MIT
