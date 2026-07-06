# Changelog

All notable changes to CoTrader are documented here.

## [0.2.0] — 2026-07-12

### Added
- **8 Core Agents** — Compressed 21 agents to 8 with real reasoning
  - Analysis, Planning, Decision, Implementation, Observation, Risk, Psychology, Evolution
  - Each agent has `reason()` method producing `ReasoningChain`
  - All agents integrate ML, neurosymbolic, and NLP predictions

- **ML Integration (cotrader-ml)** — 5 machine learning models
  - Regime Classifier (MLP): Market regime detection
  - Signal Scorer (MLP): Trade profitability prediction
  - Win Probability (Logistic): Dynamic Kelly sizing
  - Pattern Detector (CNN): Multi-candle pattern recognition
  - Strategy Selector (Weighted): Best strategy per regime
  - 48-feature unified vector for all models
  - Background trainer (30min interval, retrains after 50+ episodes)

- **Neurosymbolic Layer (cotrader-neurosymbolic)** — Formal verification
  - 10 symbolic trading rules with priorities
  - Rule learner discovers new rules from outcomes
  - Kronos forecast validation
  - Rules persist to SQLite database
  - Integrated into Decision agent

- **NLP Engine (cotrader-nlp)** — Text understanding
  - Context-aware sentiment analysis
  - Named Entity Recognition (assets, people, orgs)
  - Event extraction and classification
  - Document summarization
  - Entity relationship extraction
  - Ollama integration (llama3.2:3b)

- **Neurosymbolic Rule Persistence**
  - New `neurosymbolic_rules` table in SQLite
  - Rules persist across restarts
  - Decision agent loads learned rules on startup
  - Rule usage stats tracked (applied, correct counts)

### Changed
- **Architecture** — 21 agents → 8 compressed agents
  - Removed `cotraders` crate (21 agents)
  - Removed `rat-pipeline` crate (unused)
  - All agent logic in `cotrader-autonomous/src/agents/`
  - Single source of truth for all trading logic

- **MarketRegime** — Moved to `rat-core` to avoid circular dependencies
  - `cotrader-autonomous` re-exports via `pub use cotrader_core::MarketRegime`

### Removed
- `cotraders/` — Old 21-agent crate (replaced by 8 agents)
- `rat-pipeline/` — Unused standalone binary
- `.worktrees/` — Old reference worktree
- `nextstep.md` — Reference file
- `tri_level_reasoning.jsonl` — Debug data
- `docs/` — Old documentation

## [0.1.0] — 2026-07-01

### Added
- Initial release with 21-agent architecture
- 5-layer pipeline (HardRulesGate → Analysis → Debate → Decision → Execution)
- 29 risk rules
- 6 broker adapters (5paisa, Alpaca, AngelOne, Binance, Upstox, Zerodha)
- Real-time Binance WebSocket streaming
- TUI dashboard
- Agentic memory system
- Self-evolution loop
