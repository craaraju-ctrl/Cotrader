# RAT Agent — Complete System Audit
*Generated: 2026-06-29*

---

## Table of Contents
1. [System Overview](#1-system-overview)
2. [Component Inventory](#2-component-inventory)
3. [Architecture & Data Flow](#3-architecture--data-flow)
4. [Memory System Deep Dive](#4-memory-system-deep-dive)
5. [Performance Analysis](#5-performance-analysis)
6. [Event Sourcing & Logging](#6-event-sourcing--logging)
7. [Known Gaps & Issues](#7-known-gaps--issues)
8. [Memory Technology Comparison](#8-memory-technology-comparison)
9. [Port Configuration](#9-port-configuration)
10. [Storage Locations](#10-storage-locations)

---

## 1. System Overview

**RAT Agent** is a unified workspace at `/Users/varma/Desktop/Trading/RAT Agent/` containing three integrated projects:

| Project | Purpose | Port | Language |
|---------|---------|------|----------|
| **rat** | Autonomous trading brain (19 crates) | 8082 | Rust |
| **exchange** | Matching engine + exchange backend | 8080 | Rust |
| **memory** | Shared 4-tier memory server | 3111 | Rust |

**External Dependencies:**
- PostgreSQL (port 5432) — Exchange persistence
- Ollama (port 11434) — LLM inference + embeddings
- Kronos (port 8000) — Optional time-series forecasts

---

## 2. Component Inventory

### 2.1 RAT Crates (19 workspace members)

| Crate | Purpose | Status |
|-------|---------|--------|
| rat-core | Disciplined rules, LLM, memory, broker trait, backtest | Active |
| rat-autonomous | 5-layer pipeline, debate, skills, reflection | Active |
| rat-orchestrator | Temporal loops, HTTP API, WS broadcast | Active |
| rat-runtime | Unified event-driven engine, policy cache, world model | Active |
| rat-tui | Terminal UI (ratatui 0.30, component architecture) | Active |
| rat-server | Production Axum HTTP server | Active |
| rat-eventbus | NATS/InMemory event bus | Active |
| rat-exchange-client | Connects to Tredo Exchange | Active |
| rat-memory-client | Connects to Agentic Memory | Active |
| rat-broker-alpaca | US equities/crypto broker | Active |
| rat-broker-zerodha | India equities broker | Active |
| rat-broker-binance | Binance spot broker | Active |
| rat-broker-5paisa | India broker | Active |
| rat-broker-angelone | India broker | Active |
| rat-broker-upstox | India broker | Active |
| rat-market-data | Live price feeds (Binance/Yahoo) | Active |
| rat-metrics | Prometheus metrics + alerting | Active |
| rat-watchdog | Emergency watchdog (UDP) | Active |
| rat-compliance | Regulatory compliance | Active |

### 2.2 Exchange Components

| Module | File | Lines | Purpose |
|--------|------|-------|---------|
| Matching Engine | exchange/src/engine/matching.rs | 563 | Price-time priority order matching |
| Order Book | exchange/src/engine/orderbook.rs | 319 | BTreeMap-based depth with FIFO |
| Risk Engine | exchange/src/engine/risk.rs | ~400 | 5-step pre-trade validation |
| Futures Engine | exchange/src/engine/futures.rs | ~500 | 125x leverage, liquidation |
| Candles | exchange/src/engine/candles.rs | ~200 | OHLCV aggregation |
| WAL Store | exchange/src/storage/wal.rs | ~300 | PostgreSQL WAL persistence |
| REST API | exchange/src/api/routes.rs | 295 | 30+ endpoints |
| WebSocket | exchange/src/api/ws.rs | ~250 | Real-time depth/trades |
| Orchestra | exchange/src/orchestra/ | ~800 | Multi-agent orchestration |

### 2.3 Memory Components (26 modules)

| Layer | Module | Purpose |
|-------|--------|---------|
| Core | store | SQLite WAL + FTS5 + sqlite-vec |
| Core | types | All data structures |
| Core | errors | Error handling |
| Core | metrics | Prometheus instrumentation |
| Tiers | tiers | 4-level hierarchy orchestrator |
| Tiers | tiers::WorkingMemory | In-memory LRU buffer with TTL |
| Tiers | tiers::PromotionEngine | Auto-promote/demotion |
| Search | vector | Cosine + binary Hamming |
| Search | graph | Directed BFS, path finding |
| Search | experts::RetrievalExpert | Hybrid vector+graph |
| Search | rag | Chunking + embeddings |
| Memory | temporal | Ebbinghaus decay, versioned facts |
| Memory | reasoning | Chain-of-thought chains |
| Memory | reflection | Inner monologue, self-assessment |
| Lifecycle | consolidation | Importance, dedup, merge |
| Lifecycle | evolution | Sleep cycles, tier tuning |
| Lifecycle | staleness | Expired record cleanup |
| API | api | 55+ Axum endpoints |
| API | mcp | MCP protocol server |
| API | openai_compat | OpenAI-compatible proxy |
| Client | client | HTTP client |
| Client | llm | LLM abstraction |
| Infra | resilience | Circuit breaker + retry |
| Infra | cache | Policy cache with TTL |
| Infra | context | Token-budgeted window |
| Embed | embed_openai | OpenAI embeddings |
| Embed | embed_cohere | Cohere embeddings |

---

## 3. Architecture & Data Flow

### 3.1 Trade Decision Pipeline (5-Layer)

```
Price Tick → HardRulesGate (17 checks) → Identifier+Verifier (15+ skills)
→ DebateLayer (3 rounds: Bull/Bear/Synthesize) → Judge → SuperIntelligence
→ Execution (Kelly sizing + portfolio checks)
```

### 3.2 Exchange Order Flow

```
Order → Risk check → Lock funds → WAL write → Match (BTreeMap)
→ Persist trades → Settle balances → Broadcast (WS/SSE/EventBus)
```

### 3.3 Self-Evolution Loop

```
Trade → Outcome logged → Reflector scores regret (0-1)
→ MetaControl reviews clusters → Rule adaptation
→ Policy cache learns → Future cycles use improved rules
```

### 3.4 Memory Ingestion

```
Event → UUIDv7 → Namespace → Tier assignment → Importance scoring (0-1)
→ Optional embedding (Ollama) → SQLite persist → FTS5 update → Optional graph edge
```

### 3.5 Memory Retrieval

```
Query → sqlite-vec k-NN (binary Hamming) → Dense cosine rerank
→ Graph connectivity boost → Ebbinghaus temporal decay → Combined score → Top-k
```

### 3.6 Inter-Service Communication

| From | To | Protocol | Purpose |
|------|----|----------|---------|
| rat → Exchange | HTTP + WS | Market data, orders |
| rat → Memory | REST API | Store/recall episodes |
| rat → Ollama | HTTP | LLM inference, embeddings |
| Exchange → RAT | WS (RAT channel) | Trade executions |
| EventBus (NATS) | All | MarketPrice, Signal, Health |

---

## 4. Memory System Deep Dive

### 4.1 Four-Tier Architecture

| Tier | Max Records | TTL | Promotion Threshold | Demotion Threshold |
|------|-------------|-----|--------------------|--------------------|
| Working | 100 | 1 hour | 0.5 | 0.1 |
| Episodic | 10,000 | 30 days | 0.7 | 0.2 |
| Semantic | 100,000 | None | 0.85 | 0.15 |
| Procedural | 10,000 | None | 0.95 | 0.1 |

### 4.2 SQLite Schema (13 Tables)

| Table | Columns | Purpose |
|-------|---------|---------|
| records | id, content, content_type, metadata_json, embedding, timestamp, tier, importance, access_count, last_accessed, ttl_seconds, parent_id, valid_from, valid_to, sys_start, sys_end, tags_json, namespace_id | Core memory records |
| records_fts | FTS5 virtual table | Full-text search |
| tier_config | tier, max_records, default_ttl_secs, promotion_threshold, demotion_threshold, auto_promote | Tier settings |
| graph_edges | edge_id, source_id, target_id, relation_type, weight, metadata_json, created_at | Knowledge graph |
| reasoning_chains | chain_id, goal, steps_json, final_conclusion, overall_confidence, success, consulted_records, tags_json, created_at, duration_ms | Chain-of-thought |
| expert_opinions | opinion_id, expert_type, target_record_id, recommendation, reasoning, confidence, action_taken, created_at | Expert recommendations |
| evolution_events | event_id, event_type, description, previous_value, new_value, confidence, timestamp | System adaptation log |
| reflections | reflection_id, topic, monologue, conclusion, planned_actions, outcome, confidence, tags_json, created_at | Self-reflection journal |
| self_assessments | assessment_id, memory_quality_score, coherence_score, staleness_score, diversity_score, overall_health, issues_detected, recommendations, created_at | Health metrics |
| temporal_facts | fact_id, content, content_type, valid_from, valid_to, sys_start, sys_end, version, previous_version_id, decay_score, recall_count, last_recalled, importance, metadata_json | Versioned facts with decay |
| context_blocks | block_id, label, content, pinned, priority, max_tokens, current_tokens, last_updated, metadata_json | Context window |
| context_summaries | summary_id, topic, summary, source_block_ids, created_at | Compressed context |
| namespaces | namespace_id, name, description, owner, read_parents, write_children, created_at | Multi-tenant isolation |
| schema_migrations | version, applied_at | Schema versioning |

### 4.3 Importance Scoring Formula

```
score = (access_count / 100) × 0.30
      + e^(-age_days) × 0.25
      + has_embedding × 0.10
      + (content_length / 1000) × 0.10
      + type_weight × 0.10
      + (graph_connections / 20) × 0.10
      + (expert_endorsements / 5) × 0.05
      clamp(0.0, 1.0)
```

### 4.4 Temporal Decay (Ebbinghaus)

```
decay_score = e^(-λ × age_hours)
λ = 0.01 (configurable)
Each recall: decay_score = min(1.0, current + 0.2)
```

---

## 5. Performance Analysis

### 5.1 Memory Footprint

| Component | RAM | Notes |
|-----------|-----|-------|
| RAT binary | ~7 MB | Lean Rust binary |
| Exchange | ~15 MB | PostgreSQL WAL, orderbook |
| Agentic Memory | ~30 MB | SQLite + vector search |
| Ollama nemotron-3-nano | ~2.8 GB | Model weights |
| Ollama nomic-embed-text | ~274 MB | Embedding model |
| PostgreSQL | ~50 MB | Shared buffers |
| **Total** | **~3.9 GB** | On 8 GB machine |

### 5.2 Ollama KV Cache

| Model | KV Cache | Context Window |
|-------|----------|----------------|
| nemotron-3-nano:4b | ~512 MB | 4096 tokens |
| nomic-embed-text | ~64 MB | 8192 tokens |

### 5.3 Trade Execution Speed

| Path | Latency | Notes |
|------|---------|-------|
| LLM decision (Ollama CPU) | ~25s timeout | Main bottleneck |
| Deterministic fallback | <100ms | Rules-only, no LLM |
| Policy cache hit | ~100ms | Skip entire pipeline |
| Order execution | 10-50ms | Internal matching |
| Full pipeline (per symbol) | ~30s with LLM | ~3-5s deterministic |

---

## 6. Event Sourcing & Logging

### 6.1 Event Layers

| Layer | Storage | Events |
|-------|---------|--------|
| Exchange WAL | PostgreSQL wal_sequence | OrderPlaced, TradeExecuted, OrderCancelled |
| RAT EventBus | NATS or in-memory | MarketPrice, Signal, Execution, COT, Health |
| Episode Store | SQLite | Closed trades + regret scores |
| Graph RAG | In-memory directed graph | Symbol→Regime→Direction→Outcome |
| Vector Memory | LanceDB embeddings | Semantic episode recall |
| Agentic Memory | SQLite + sqlite-vec | 4-tier cross-session learning |

### 6.2 Centralized Logging

| Component | Tool | Endpoint |
|-----------|------|----------|
| All services | tracing (structured) | stdout/file |
| Metrics | Prometheus | :9730/metrics |
| Alerts | rat-metrics | Slack/Email/Telegram |
| Request logs | Agentic Memory | Latency-tracked per endpoint |

---

## 7. Known Gaps & Issues

### 7.1 Critical

| Issue | Location | Impact |
|-------|----------|--------|
| `get_best_ask_price()` returns None | exchange/src/engine/risk.rs | Market buy uses $100K fallback |
| Recovery doesn't restore balances | exchange/src/storage/wal.rs | Post-crash state inconsistent |

### 7.2 High

| Issue | Location | Impact |
|-------|----------|--------|
| Dual debate systems | debate.rs + debate_layer.rs | Redundant contradictory signals |
| Tauri desktop build broken | target/ cache | No desktop UI |
| `estimate_correlation()` returns 0.0 | rat-runtime/src/portfolio_reasoner.rs | Portfolio heat inaccurate |

### 7.3 Medium

| Issue | Location | Impact |
|-------|----------|--------|
| No distributed tracing | System-wide | Cross-service latency invisible |
| No event replay | System-wide | Can't reprocess historical events |
| No RBAC on memory API | memory/src/api.rs | Open access in production |
| No dead letter queue | System-wide | Failed events silently dropped |

### 7.4 Low

| Issue | Location | Impact |
|-------|----------|--------|
| No async event bus (NATS) | memory | REST polling only |
| Lock-free maps missing | tiers::WorkingMemory | Mutex contention under load |
| docker-compose missing PostgreSQL | exchange/docker-compose.yml | Local dev friction |

---

## 8. Memory Technology Comparison

| Technology | Stars | Accuracy | Latency | Key Feature |
|------------|-------|----------|---------|-------------|
| **Mem0 v3** | 59.7k | 91.6 (LoCoMo) | ~1s | Entity linking + temporal reasoning |
| **LangMem** | 1.5k | — | ~200ms | LangGraph native |
| **Zep** | 6k+ | — | ~500ms | Temporal knowledge graphs |
| **Letta** | 13k+ | — | ~800ms | Self-editing memory |
| **Current RAT** | custom | — | <1ms | Trading-optimized, regret-driven |

### Mem0 v3 vs RAT Current System

| Capability | RAT (Current) | Mem0 v3 |
|------------|---------------|---------|
| Search | Keyword + vector | Semantic + BM25 + entity |
| Entity linking | No | Yes |
| Temporal reasoning | No | Yes |
| Auto-consolidation | Manual | Background agent |
| Latency | <1ms | ~1s |
| Trading regret scoring | Yes | No |

---

## 9. Port Configuration

| Service | Port | URL |
|---------|------|-----|
| Tredo Exchange | 8080 | http://localhost:8080 |
| RAT | 8082 | http://localhost:8082 |
| Agentic Memory | 3111 | http://localhost:3111 |
| Ollama | 11434 | http://localhost:11434 |
| Kronos | 8000 | http://localhost:8000 |
| PostgreSQL | 5432 | localhost:5432 |

### Health Endpoints

| Service | Path |
|---------|------|
| Agentic Memory | GET /health |
| Tredo Exchange | GET /api/v1/health |
| RAT | GET /health |

---

## 10. Storage Locations

| Data | Location | Type |
|------|----------|------|
| Exchange orders | PostgreSQL `orders` table | Persistent |
| Exchange trades | PostgreSQL `trades` table | Persistent |
| Exchange WAL | PostgreSQL `wal_sequence` table | Persistent |
| Exchange balances | PostgreSQL + in-memory HashMap | Both |
| RAT portfolio | redb (rat.redb) | Persistent |
| RAT episodes | SQLite (rat_history.db) | Persistent |
| RAT vectors | LanceDB + Ollama | Persistent |
| RAT knowledge graph | petgraph (in-memory) | Volatile |
| RAT policy cache | JSON file on disk | Persistent |
| Memory records | SQLite (memory.db) | Persistent |
| Memory vectors | sqlite-vec extension | Persistent |
| Memory graph | SQLite graph_edges | Persistent |
| Memory temporal | SQLite temporal_facts | Persistent |
| Memory reasoning | SQLite reasoning_chains | Persistent |
| Memory reflections | SQLite reflections | Persistent |
| Working memory | Mutex<HashMap> | Volatile |
| Policy cache | HashMap with TTL | Volatile |
| Circuit breaker | AtomicBool | Volatile |

---

*End of Audit*
