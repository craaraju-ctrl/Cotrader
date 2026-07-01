# RAT Agent — Autonomous Trading System

## Overview

RAT (Risk-adjusted Autonomous Trading) is a multi-agent autonomous trading system built in Rust. It uses a 21-agent hierarchical structure modeled after real trading firms, with 29 risk rules, real-time market data, and persistent memory.

## Architecture

```
Market Data → Indicators → Signals → Risk → Execution → Memory
     ↓           ↓           ↓        ↓        ↓          ↓
  Binance     RSI/MACD    BUY/SELL   29     PaperBroker  SQLite
  WebSocket   ATR/BB      HOLD       rules  (simulated)  (agent)
              Stoch
```

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

## Project Structure

```
crates/
├── rat-core/          Core types, memory, paper engine
├── rat-autonomous/    Pipeline, 29 rules, all agents
├── rat-agents/        21 agents with memory/skills/rules/tools
├── rat-pipeline/      Runner, backtest, workflows
├── rat-brokers/       Multi-broker trait + registry + sandbox + engine
├── rat-indicators/    RSI, MACD, ATR, Bollinger, Stochastic + 11 more
├── rat-rules/         29 rules with skills/rules/tools
├── rat-strategies/    10 trading strategies
├── rat-risk/          6 risk components
├── rat-skills/        16 agent skills
├── rat-memory/        Agentic memory system
├── rat-tui/           Terminal dashboard
├── rat-regime/        5 regime detectors
├── rat-feeds/         6 data feed types
├── rat-sentiment/     5 sentiment sources
├── rat-patterns/      5 pattern types
└── rat-orchestrator/  Service orchestration
```

## 21 Agents

| Level | Agent | Role |
|-------|-------|------|
| 0 | Rat | Chief Investment Officer |
| 1 | Head of Trading | Manages trading desks |
| 1 | Head of Research | Generates alpha signals |
| 1 | Head of Risk | Manages all risk |
| 1 | Head of Operations | Post-trade and admin |
| 1 | System Architect | Technology infrastructure |
| 2 | Equity Trader | NIFTY, BANKNIFTY, stocks |
| 2 | Crypto Trader | BTC, ETH, altcoins |
| 2 | Execution Desk | Order routing, fill quality |
| 2 | Quant Researcher | Statistical models |
| 2 | Technical Analyst | Charts and patterns |
| 2 | Fundamental Analyst | Valuation and earnings |
| 2 | Market Risk Manager | VaR, stress testing |
| 2 | Compliance Officer | Regulatory compliance |
| 2 | Portfolio Administrator | Reconciliation |
| 2 | Journal Keeper | Trade journal and lessons |
| 2 | Data Engineer | Data pipelines |
| 2 | Backtest Engine | Strategy testing |
| 2 | Sentiment Analyst | News and social sentiment |
| 2 | Regime Detector | Market regime classification |
| 2 | Money Manager | Position sizing and allocation |

## 29 Risk Rules

| Priority | Rules |
|----------|-------|
| Critical (6) | trading_enabled, daily_drawdown, red_folder, session_timing, max_absolute_drawdown, black_swan_detector |
| High (10) | portfolio_heat, loss_circuit_breaker, max_daily_trades, cooldown, kelly_sizing, vol_adjusted_stops, liquidity_check, exposure_concentration, order_size_limits, margin_utilization |
| Medium (9) | regime_safety, confluence_minimum, correlation_heat, mae_tracking, session_risk_budget, time_of_day_filter, news_event_proximity, win_streak_greed, loss_streak_recovery |
| Low (4) | max_positions_per_symbol, max_total_positions, symbol_frequency_cap, minimum_hold_time |

## Indicators

RSI, MACD, ATR, Bollinger Bands, Stochastic, Volume, OBV, ADX, CCI, Williams %R, Ichimoku, VWAP, Pivot Points, Fibonacci, Keltner Channels, Donchian Channels

## Dependencies

- Rust 1.96.0+
- Tokio (async runtime)
- Axum 0.8 (HTTP)
- reqwest 0.13 (HTTP client)
- ratatui 0.30 (TUI)
- agentic-memory (persistent storage)
