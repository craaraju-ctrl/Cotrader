# RAT Agent — Frequently Asked Questions

## General

**Q: What is RAT Agent?**
A: RAT (Risk-adjusted Autonomous Trading) is a multi-agent autonomous trading system built in Rust with 21 agents, 29 risk rules, and real-time market data.

**Q: Is it safe to run with real money?**
A: Default mode is paper trading. Live mode requires explicit configuration and API keys.

**Q: What markets does it support?**
A: Crypto (Binance), Indian equities (Zerodha), and can be extended to other brokers.

## Technical

**Q: How fast is the pipeline?**
A: Each cycle takes ~100ms (indicators + signals + risk check). Memory operations add ~10ms.

**Q: Can I add custom indicators?**
A: Yes. Create a new crate in `rat-indicators/` following the existing pattern (indicator.rs + skills.rs + rules.rs + tools.rs).

**Q: How do I add a new broker?**
A: Implement the `Broker` trait in `rat-brokers/src/traits/`, then create a new adapter file.

**Q: How does the memory system work?**
A: Each agent has its own namespace in agentic-memory (SQLite + vectors). Agents store decisions and recall similar past situations.

## Trading

**Q: How are signals generated?**
A: RSI (14-period) + MACD (12,26,9) indicators are combined with weighted scoring. High confidence (>55%) triggers SELL, low (<45%) triggers BUY.

**Q: How does risk management work?**
A: 29 rules evaluated per symbol: Critical (4), High (10), Medium (9), Low (4). Rules check drawdown, position size, exposure, volatility, and more.

**Q: Can I run multiple strategies?**
A: Yes. Each strategy runs in its own pipeline instance with separate symbols and configuration.
