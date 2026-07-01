# RAT Agent — Changelog

## [0.1.0] - 2026-06-30

### Added
- 21-agent hierarchical trading system
- 29 risk rules across 4 priority levels
- 16 technical indicators (RSI, MACD, ATR, Bollinger, etc.)
- 10 trading strategies
- Real-time Binance API integration
- Paper trading engine
- Backtesting framework
- Terminal UI dashboard
- Agentic memory system
- Event-driven architecture via Tokio broadcast channels
- Multi-broker abstraction layer
- Portfolio analytics (Sharpe, Sortino, drawdown)
- Multi-asset support (Crypto, Equity, F&O)
- Monitoring dashboard
- Telegram/email alerts

### Technical
- Rust 1.96.0 edition
- Tokio 1.52 async runtime
- Axum 0.8 HTTP framework
- reqwest 0.13 HTTP client
- ratatui 0.30 TUI
- agentic-memory for persistent storage
- tokio-tungstenite for WebSocket streaming

### Architecture
- Strategy Design → Broker Registry → Sandbox → Engine → Paper/Live
- 5-phase pipeline: Data → Signal → Risk → Execute → Feedback
- Event-driven inter-agent communication
- Deterministic decision-making (no LLM dependency)
