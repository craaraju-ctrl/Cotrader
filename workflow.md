# CoTrader — Pipeline Workflow Specification

> Version: 2.0  
> Last Updated: 2026-07-10  
> Classification: Internal Engineering Specification

---

## 1. Pipeline Overview

The CoTrader pipeline implements a 4-layer parallel validation system that processes market data and generates trade decisions. Each layer operates independently, and their signals are combined via weighted consensus with a 2-of-4 agreement gate.

---

## 2. End-to-End Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         MARKET DATA INPUT                                   │
│                     (Price Ticks, OHLCV, News)                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 0: PRE-FLIGHT CHECKS                               │
│  ├─ Hard Rules Gate (22 rules)                                              │
│  ├─ VaR Emergency Gate (Cornish-Fisher) ★                                   │
│  └─ If any Critical/High rule fails → BLOCK (no trade)                      │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 1: 4-LAYER PARALLEL VALIDATION                     │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐           │
│  │   RULES     │ │  ML/SIGNAL  │ │   CHRONOS   │ │  SENTIMENT  │           │
│  │   (35%)     │ │   (25%)     │ │   (25%)     │ │   (15%)     │           │
│  │             │ │             │ │             │ │             │           │
│  │ Pivot/Conf  │ │ Deterministic│ │ T5 Forecast │ │ FinBERT     │           │
│  │ + Regime    │ │ Signal      │ │ 64-step     │ │ [-1, +1]    │           │
│  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘           │
│         │               │               │               │                   │
│         └───────────────┴───────────────┴───────────────┘                   │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 2: WEIGHTED CONSENSUS                              │
│  consensus = w_rules × sig_rules + w_ml × sig_ml                           │
│            + w_chronos × sig_chronos + w_sentiment × sig_sentiment          │
│                                                                             │
│  action = BUY if consensus > 0.15                                           │
│         = SELL if consensus < -0.15                                         │
│         = HOLD otherwise                                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 3: 2-OF-4 AGREEMENT GATE                           │
│  Count layers agreeing with consensus direction                             │
│  IF ≥ 2 layers agree → proceed to arbitration                               │
│  IF < 2 layers agree → force HOLD                                           │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 4: LLM ARBITRATION                                 │
│  Escalation Gate:                                                           │
│  ├─ Direction conflict (BUY vs SELL) → FIRE LLM                             │
│  ├─ High volatility (ATR > 1.5x) → FIRE LLM                                │
│  ├─ Volatile regime → FIRE LLM                                              │
│  └─ All layers different → FIRE LLM                                         │
│                                                                             │
│  IF fired:                                                                  │
│  ├─ Build structured prompt with all 4 layer signals + sentiment            │
│  ├─ Run Llama-3.2-3B (Candle GGUF or Ollama)                               │
│  └─ Parse DECISION/CONFIDENCE/REASONING                                     │
│  IF not fired:                                                              │
│  └─ Use weighted consensus directly                                         │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 5: EXECUTION                                       │
│  ├─ Position sizing (Kelly criterion)                                       │
│  ├─ Stop-loss/Take-profit calculation                                       │
│  ├─ Order execution (paper or live)                                         │
│  └─ Episode capture for learning                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Layer Details

### 3.1 Layer 1: Rules (35% Weight)

**Source:** `check_rules_layer()` in `tri_level_validator.rs`

**Inputs:**
- OHLCV snapshot (current bar: high, low, close)
- Pivot method (Classic/Woodie/Fibonacci)
- Market regime (TrendingBull/TrendingBear/Ranging/Volatile)

**Computation:**
```rust
let pivots = calculate_pivot_points(high, low, close, pivot_method);
let conf_score = calculate_confluence_score(&context, &pivots);
let regime_bias = match regime {
    TrendingBull => 0.3,
    TrendingBear => -0.3,
    _ => 0.0,
};
let signal = ((confluence + conf_score) / 2.0 - 0.5) * 2.0 + regime_bias;
```

**Output:** `LayerSignal { signal: [-1, +1], action: "BUY"/"SELL"/"HOLD" }`

### 3.2 Layer 2: ML/Signal (25% Weight)

**Source:** `check_llm_layer()` in `tri_level_validator.rs`

**Inputs:**
- Confluence score
- Trend label
- Multi-timeframe context

**Computation:**
```rust
let signal = if confluence > 0.7 && trend == "bullish" { 0.7 }
    else if confluence < 0.3 && trend == "bearish" { -0.7 }
    else { 0.0 };
```

**Output:** `LayerSignal { signal: [-1, +1], action: "BUY"/"SELL"/"HOLD" }`

### 3.3 Layer 3: Chronos (25% Weight)

**Source:** `check_trend_layer()` in `tri_level_validator.rs`

**Inputs:**
- OHLCV closing prices (2048 timesteps)
- Chronos-Bolt T5 model

**Computation:**
```rust
// If Chronos model loaded:
let (dir, conf, change_pct) = forecast_trend(model, &closes);

// Fallback (no model):
let overall_pct = (newest - oldest) / oldest;
let consistency_ratio = consistent_bars / total_bars;
let signal = (overall_pct * 20.0).clamp(-1.0, 1.0) * consistency_factor;
```

**Output:** `LayerSignal { signal: [-1, +1], action: "BUY"/"SELL"/"HOLD" }`

### 3.4 Layer 4: Sentiment (15% Weight) ★ NEW

**Source:** `check_sentiment_layer()` in `tri_level_validator.rs`

**Inputs:**
- News headlines from `agent_memory.latest_news`
- BGE-small-en-v1.5 embedding model

**Computation:**
```rust
// Keyword classification
let bullish_count = text.matches(BULLISH_KEYWORDS).count();
let bearish_count = text.matches(BEARISH_KEYWORDS).count();
let score = (bullish_count - bearish_count) / total;

// Embedding (optional)
let embedding = model.embed(text);
let similarity = cosine_similarity(embedding, reference_vectors);

// Final score
let final_score = tanh(weighted_average) * confidence;
```

**Output:** `LayerSignal { signal: [-1, +1], action: "BUY"/"SELL"/"HOLD" }`

---

## 4. Cornish-Fisher VaR Emergency Gate

### 4.1 Formula

```
Z_cf = Z_α + (Z_α² - 1) × S/6 + (Z_α³ - 3Z_α) × K/24 - (2Z_α³ - 5Z_α) × S²/36

Where:
  Z_α = -2.326 (99% confidence)
  S   = rolling skewness
  K   = rolling excess kurtosis

VaR  = -(μ + Z_cf × σ)
```

### 4.2 Emergency Gate Logic

```
IF VaR_alpha > risk_tolerance (5%)
   OR volatility_ratio > volatility_cap (3x)
THEN:
   Force ALL layer signals to "HOLD"
   Override any bullish signals
   Log: "[VaR] ⚠ EMERGENCY TRIGGERED"
```

### 4.3 Edge Cases

| Condition | Handling |
|-----------|----------|
| Zero volatility | VaR = 0, continue normally |
| Insufficient data (< 3 bars) | Skip VaR check |
| Extreme skewness/kurtosis | Clamp Z_cf to [-2.0, 2.0] |

---

## 5. LLM Arbitration

### 5.1 Escalation Gate

The LLM fires only when:

1. **Direction conflict:** At least one BUY and one SELL among 4 layers
2. **High volatility:** ATR ratio > 1.5x normal
3. **Volatile regime:** Market regime is "Volatile"
4. **Uniform disagreement:** All 4 layers give different directions

### 5.2 Prompt Format

```
<|begin_of_text|><|start_header_id|>system<|end_header_id|>
You are a trading signal arbitrator...
<|eot_id|><|start_header_id|>user<|end_header_id|>

Reconcile these signals for BTC @ $58500.00:

Rules Layer:   BUY (sig=+0.450, conf=0.72)
ML Layer:      BUY (sig=+0.380, conf=0.65)
Chronos Trend: SELL (sig=-0.220, conf=0.58)
CNN Pattern:   StrongBullish (conf=0.68)
Sentiment:     +0.450 (bullish, conf=0.72)
Market Regime: TrendingBull
Volatility:    1.25x normal

Output exactly:
DECISION: BUY/SELL/HOLD
CONFIDENCE: 0.0-1.0
REASONING: (one sentence)
<|eot_id|><|start_header_id|>assistant<|end_header_id|>
```

### 5.3 Backends

| Backend | Latency | RAM | When to Use |
|---------|---------|-----|-------------|
| Ollama HTTP | ~100ms | 0 (separate process) | Production |
| Candle GGUF | ~6s | ~2GB | Offline/testing |
| Consensus fallback | <1ms | 0 | No LLM configured |

---

## 6. Trust Weight Learning

### 6.1 Update Rule

After each trade close:

```rust
for (layer, prediction) in layer_predictions {
    let correct = prediction_matches_outcome(prediction, actual_outcome);
    
    if correct {
        let accuracy = 1.0 - (prediction - outcome).abs() / 2.0;
        weight *= 1.0 + LEARNING_RATE * accuracy;
    } else {
        let regret = (prediction - outcome).abs() / 2.0;
        weight *= 1.0 - LEARNING_RATE * regret;
    }
    
    weight = weight.clamp(0.10, 0.60);
}

// Normalize to sum to 1.0
weights.normalize();
```

### 6.2 Default Weights

| Layer | Initial Weight | Min | Max |
|-------|---------------|-----|-----|
| Rules | 35% | 10% | 60% |
| ML/Signal | 25% | 10% | 60% |
| Chronos | 25% | 10% | 60% |
| Sentiment | 15% | 10% | 60% |

---

## 7. Data Flow

### 7.1 OHLCV Snapshot

All 4 layers receive the **identical** OHLCV snapshot captured at pipeline start:

```rust
let snapshot = OhlcvSnapshot::capture(symbol, &state).await;

// All layers use the same snapshot
let (rules_sig, ml_sig, trend_sig, sentiment_sig) = tokio::join!(
    check_rules_layer(snapshot),
    check_llm_layer(snapshot),
    check_trend_layer(snapshot),
    check_sentiment_layer(news),
);
```

### 7.2 MutexGuard Semantics

All state access follows "brief lock, snapshot, lock-free dispatch":

```rust
// Phase 1: Snapshot under lock (brief)
let backend_snapshot = {
    let guard = LLM_BACKEND.lock().unwrap();
    // Extract owned data
    (has_candle, ollama_config)
};
// guard dropped here

// Phase 2: Async work without lock
if let Some((url, model)) = backend_snapshot {
    ollama_arbitrate(&url, &model).await;
}
```

---

## 8. Error Handling

### 8.1 Layer Degradation

| Scenario | Handling |
|----------|----------|
| Chronos model not loaded | Fallback to simple OHLCV trend |
| Sentiment model not available | Neutral signal (0.0) |
| LLM unavailable | Consensus fallback |
| All layers unavailable | Force HOLD |

### 8.2 Agreement Gate

```
IF available_layers < 2:
    Force HOLD (degraded mode)
IF available_layers >= 2 AND agree_count < 2:
    Force HOLD (agreement gate)
```

---

## 9. Performance Characteristics

| Operation | Latency | Notes |
|-----------|---------|-------|
| VaR computation | < 10ms | Rolling 60-bar window |
| Sentiment analysis | < 100ms | Keyword + embedding |
| 4-layer parallel | < 500ms | Tokio join! |
| LLM arbitration | < 6s | Only on conflict |
| Total pipeline | < 7s | Worst case with LLM |

---

*End of Workflow Specification*
