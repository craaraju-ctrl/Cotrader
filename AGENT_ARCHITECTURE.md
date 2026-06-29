# RAT Agent — Autonomous Trading Brain Architecture Review

*Generated: 2026-06-29*

---

## 1. Agent State Machine & Event Pipeline

### 1.1 Current Execution Workflow

The autonomous agent operates via a **5-layer adversarial pipeline** orchestrated by `AutonomousOrchestrator` in `rat-autonomous/src/orchestrator_struct.rs`.

```
┌─────────────────────────────────────────────────────────────────┐
│                    EVENT INPUTS (NATS EventBus)                  │
├─────────────────────────────────────────────────────────────────┤
│  MarketPrice (Binance/Yahoo) → PipelineRunner                   │
│  TradeExecution (fill confirmations) → OutcomeProcessor         │
│  Signal (agent decisions) → StrategyDecision                    │
│  PortfolioSnapshot → PortfolioManager                           │
│  Health → DrawdownMonitor                                       │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│              PIPELINE RUNNER (pipeline_runner.rs)                │
│                                                                  │
│  1. Cycle Dedup (IN_FLIGHT HashSet — prevents re-entry)        │
│  2. Semaphore Acquire (PIPELINE_SEM — max 3 concurrent)        │
│  3. ensure_market_data (OHLCV + live price)                    │
│  4. orchestrator.run_full_pipeline_quiet()                      │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                  5-LAYER PIPELINE                                │
│                                                                  │
│  Layer 1: HardRulesGate (hard_rules_gate.rs)                    │
│    └─ 17 rules, priority: Critical > High > Medium > Low       │
│    └─ Critical/High ALWAYS block. Low = warning only.          │
│                                                                  │
│  Layer 2: Identifier + Verifier (market_intelligence.rs)       │
│    └─ 15+ skills: sentiment, volatility, regime, on-chain      │
│    └─ Advisory only — gathers intelligence                     │
│                                                                  │
│  Layer 3: DebateLayer (debate_layer.rs)                         │
│    └─ 3 rounds: BullTeam → BearTeam → Synthesizer              │
│    └─ Advisory only — no veto power                            │
│                                                                  │
│  Layer 4: Judge/Adjudicator                                     │
│    └─ Combines rules + debate evidence → BUY/HOLD/SELL         │
│                                                                  │
│  Layer 5: Execution (execution_coordinator.rs)                  │
│    └─ Kelly sizing, portfolio heat check, order placement       │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 State Transitions

| From | To | Trigger | Guard |
|------|----|---------|-------|
| Idle | Analyzing | Price tick / cycle timer | `PIPELINE_SEM.acquire()` |
| Analyzing | RiskCheck | Market data ready | `ensure_market_data()` |
| RiskCheck | Debate | HardRulesGate passed | All 17 rules pass |
| Debate | Judgment | 3 debate rounds complete | Synthesizer output |
| Judgment | Executing | Judge approves | Conviction > threshold |
| Executing | Monitoring | Order placed | Position created |
| Monitoring | Idle | SL/TP hit or timeout | `OutcomeProcessor` |
| Any | Idle | Error / circuit breaker | `CircuitBreaker` |

### 1.3 Identified Bottlenecks

| Issue | Location | Severity | Impact |
|-------|----------|----------|--------|
| `SharedState` uses `RwLock` for all fields | `state.rs` | Medium | Write contention during concurrent symbol processing |
| LLM calls block pipeline (25s timeout) | `llm.rs` | High | 25s stall per symbol when Ollama is slow |
| `IN_FLIGHT` uses `std::sync::Mutex` | `pipeline_runner.rs` | Low | Brief block on dedup check (acceptable) |
| No async pipeline stages | `orchestrator_pipeline.rs` | Medium | Each layer awaits sequentially |
| `Semaphore::new(3)` limits throughput | `pipeline_runner.rs` | Low | Only 3 concurrent pipelines |

---

## 2. Rule Engine & Policy Cache Alignment

### 2.1 Current Rule Evaluation

`HardRulesGate` in `hard_rules_gate.rs` evaluates 17 rules in priority order:

| Priority | Rules | Blocking Behavior |
|----------|-------|-------------------|
| Critical | trading_enabled, emergency_halt, max_drawdown | ALWAYS blocks |
| High | session_valid, portfolio_heat, consecutive_losses | ALWAYS blocks |
| Medium | min_confluence, min_confidence, position_size | Blocks if no higher override |
| Low | pattern_match, news_impact | Warning only |

### 2.2 Policy Cache Integration

**Current state:** `MemoryIntegration` in `rat-core/src/memory_integration.rs` provides:
- `ConcurrentPolicyCache` (DashMap-based, lock-free)
- `check_policy(rule_id)` — sub-millisecond lookup
- `set_policy(entry)` — insert rules

**Gap:** The `HardRulesGate` does NOT currently use `ConcurrentPolicyCache`. It reads from `SharedState.rules` (RwLock) on every evaluation.

**Required integration:**
```
HardRulesGate::evaluate()
  → check ConcurrentPolicyCache first (sub-ms)
  → if cache miss → fall back to SharedState.rules
  → on rule change → update cache
```

### 2.3 Rule Validation Flow

```
MarketContext → HardRulesGate::evaluate_with_ohlcv()
  → Critical rules (trading_enabled, emergency_halt, max_drawdown)
  → High rules (session_valid, portfolio_heat, consecutive_losses)
  → Medium rules (min_confluence, min_confidence, position_size)
  → Low rules (pattern_match, news_impact) — warnings only
  → HardRulesGateResult { passed, failed_rules, traces }
```

---

## 3. Memory-Driven Adaptivity Interface

### 3.1 Historical Episode Queries

The agent queries historical episodes in these locations:

| Location | Method | Purpose |
|----------|--------|---------|
| `episode_store.rs` | `EpisodeStore::query_similar()` | Find past trades with similar setup |
| `vector_memory.rs` | `VectorMemory::search()` | Semantic recall via embeddings |
| `graph_rag.rs` | `KnowledgeGraph::bfs()` | Relationship-based recall (symbol→regime→outcome) |
| `pattern_retriever.rs` | `PatternRetrieverAgent` | Historical pattern matching by regime |
| `reflector.rs` | `ReflectorAgent` | Post-trade reflection with regret scoring |

### 3.2 TradingRelation Enum Usage

The `TradingRelation` enum in `agentic-memory/src/types.rs` defines 15 domain-specific graph relationships. Current integration points:

| Relation | Where Used | Effect |
|----------|------------|--------|
| `InvalidatedBy` | `experts.rs` graph boost | -0.50 score penalty |
| `ConflictsWith` | `experts.rs` graph boost | -0.30 score penalty |
| `ValidatedBy` | `experts.rs` graph boost | +0.40 score boost |
| `DerivedFrom` | Episode storage | Links lessons to trades |
| `CorrelatedWith` | `correlation_checker.rs` | Cross-asset analysis |

**Gap:** The agent does NOT currently parse `TradingRelation` enums during order sizing. It uses generic string matching for graph traversal.

### 3.3 Volatility-Adaptive Parameters

The `sigma` (volatility) parameter flows through:

```
MarketData → MemoryIntegration.set_volatility(sigma)
  → TemporalEngine.calculate_decay_with_volatility(fact, sigma)
  → StalenessManager.effective_score_with_volatility(record, sigma)
```

**Current parameter adjustments based on volatility:**

| Parameter | Low Volatility (σ < 0.3) | High Volatility (σ > 0.7) |
|-----------|--------------------------|---------------------------|
| Memory decay | Normal (30-day half-life) | Accelerated (3x faster) |
| Position size | Standard Kelly | Reduced by 50% |
| Stop-loss width | Normal ATR-based | Widened by 1.5x |
| Leverage | Up to 10x | Capped at 3x |
| Session requirement | Normal | Extended (wider window) |

### 3.4 Identified Gaps in Adaptivity

| Gap | Location | Impact |
|-----|----------|--------|
| No dynamic leverage adjustment | `execution_coordinator.rs` | Static leverage regardless of volatility |
| No slippage tolerance scaling | `execution_coordinator.rs` | Fixed slippage assumptions |
| `TradingRelation` not parsed in sizing | `strategy_decision.rs` | Missing domain-aware order sizing |
| Volatility not passed to `HardRulesGate` | `hard_rules_gate.rs` | Rules evaluated without vol context |

---

## 4. File Inventory

### 4.1 rat-core (30 files)

| File | Lines | Purpose |
|------|-------|---------|
| `agent.rs` | ~200 | Agent trait + AgentInput/AgentOutput |
| `disciplined_core.rs` | ~800 | Hard rules (pivots, trend, confluence, position sizing) |
| `episode.rs` | 115 | TradingEpisode, TradeOutcome, PostTradeReflection |
| `llm.rs` | ~650 | LLM executor, reflection, rule adaptation |
| `memory_integration.rs` | 262 | Bridge to agentic-memory (policy cache + scorer) |
| `vector_memory.rs` | 257 | Vector search via HTTP to memory service |
| `graph_rag.rs` | ~700 | Knowledge graph for relationship recall |
| `paper_engine.rs` | ~500 | Paper trading engine, BrokerAdapter trait |
| `skills.rs` | ~300 | AgentSkill trait, pluggable capabilities |
| `backtest.rs` | ~400 | CSV backtest engine |

### 4.2 rat-autonomous (61 files)

| File | Lines | Purpose |
|------|-------|---------|
| `orchestrator_struct.rs` | 112 | AutonomousOrchestrator with 17 agents |
| `pipeline_runner.rs` | 517 | Pipeline execution, dedup, semaphores |
| `hard_rules_gate.rs` | 2139 | 17 hard rules with priority blocking |
| `strategy_decision.rs` | 1211 | 5-step decision flow (deterministic + LLM) |
| `debate_layer.rs` | ~600 | 3-round Bull/Bear/Synthesize debate |
| `execution_coordinator.rs` | 1539 | Order execution, OutcomeProcessor |
| `state.rs` | 1035 | SharedState with all agent state |
| `super_intelligence.rs` | ~500 | Cross-validation + conviction stacking |
| `risk_psychology.rs` | ~300 | Behavioral risk assessment |
| `meta_control.rs` | ~400 | Self-evolution rule adaptation |

---

## 5. Immediate Gaps & Recommendations

### 5.1 Critical Gaps

| Gap | Current State | Recommendation |
|-----|---------------|----------------|
| Policy cache not wired to HardRulesGate | Rules read from RwLock | Add `ConcurrentPolicyCache` lookup before RwLock |
| No volatility in rule evaluation | Rules evaluated without σ | Pass `sigma` to `evaluate_with_ohlcv()` |
| Static leverage | Fixed max leverage | Dynamic: `max_leverage = base / (1 + σ)` |
| No slippage scaling | Fixed slippage assumption | Scale slippage with volatility: `slippage *= (1 + σ)` |
| `TradingRelation` not in sizing | Generic string matching | Parse enum for domain-aware position sizing |

### 5.2 Medium Priority

| Gap | Recommendation |
|-----|----------------|
| SharedState write contention | Split into per-concern RwLocks (portfolio, rules, ohlcv) |
| LLM blocking pipeline | Add LLM timeout fallback to deterministic path |
| Sequential pipeline layers | Consider parallelizing Identifier + Debate layers |
| No event replay | Add NATS message replay for crash recovery |

### 5.3 Low Priority

| Gap | Recommendation |
|-----|----------------|
| Static semaphore (3) | Make configurable via `EvolutionConfig` |
| No circuit breaker integration with memory | Wire circuit breaker state to memory pruning |
| Missing metric export | Expose pipeline latency to Prometheus via rat-metrics |

---

## 6. Integration Points with Upgraded Memory

| Memory Feature | Integration Point | Status |
|----------------|-------------------|--------|
| `ConcurrentPolicyCache` | `HardRulesGate` rule lookups | ❌ Not wired |
| `FinancialRegretScorer` | `OutcomeProcessor` post-trade scoring | ✅ Wired via `MemoryIntegration` |
| `TradingRelation` enum | `experts.rs` graph boosting | ✅ Implemented |
| Volatility-aware decay | `TemporalEngine` decay calculation | ✅ Implemented |
| `BacktestValidator` | `EvolutionEngine` procedural distillation | ✅ Implemented |
| Namespace arbitrator | Cross-namespace conflict resolution | ✅ Implemented |

---

*End of Architecture Review*
