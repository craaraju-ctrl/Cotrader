# RAT Agent — Complete Issues List
Generated: 2026-07-01 from live testing + web research against Freqtrade

## Live Test Results
- Pipeline runs, fetches real Binance prices (BTC $59,260, ETH $1,594)
- ALL signals HOLD at 49-51% confidence — never crosses BUY/SELL threshold
- Memory service not running (port 3111 down)
- Orchestrator not running (32 pre-existing errors)
- Kronos not running

## CRITICAL (blocks any trading)

| # | Issue | What's Wrong | Fix |
|---|-------|-------------|-----|
| C1 | Signal scoring always HOLD | Score formula produces 49-51% regardless of market conditions. Thresholds 0.40/0.60 never reached with synthetic random-walk data. | Wire real OHLCV from Binance into scoring, or widen thresholds for synthetic data |
| C2 | No real OHLCV history | generate_signal() builds 100 synthetic bars via LCG PRNG. RSI/MACD compute on fake data. | Fetch real 1h candles from Binance REST API on startup |
| C3 | Memory service disconnected | Pipeline prints warning every cycle. Trades never persisted. History lost on restart. | Start memory service, retry connection, cache locally |
| C4 | Pipeline bypasses 5-layer system | rat-pipeline binary runs independently — skips all 29 rules, debate, superintelligence, judge. Only uses basic RSI/MACD. | Route pipeline through orchestrator's run_full_pipeline_quiet |
| C5 | Paper execution is fake | execute_trade() returns hardcoded 0.001 BTC regardless of signal sizing. No portfolio tracking. | Wire into PaperEngine from rat-core |

## HIGH (major functionality gaps)

| # | Issue | What's Wrong |
|---|-------|-------------|
| H1 | rat-feeds never called | 6 feed modules exist (news, on-chain, options, calendar, social, market data) — pipeline never invokes any |
| H2 | rat-patterns never called | 5 detectors (candlestick, chart, elliott, harmonic, wyckoff) implemented — pipeline ignores them |
| H3 | rat-strategies bypassed | 10 strategies implemented — pipeline uses its own RSI/MACD instead |
| H4 | rat-risk modules unused | 6 risk calculators (concentration, correlation, drawdown, liquidity, sizing, volatility) — pipeline checks 2 conditions |
| H5 | rat-reasoning unused | 7 reasoning chains (CoT, ReAct, ToT, etc.) — pipeline uses none |
| H6 | rat-sentiment hardcoded | 5 analyzers return 0.0 — no real sentiment scoring |
| H7 | rat-regime hardcoded | 5 detectors return false — regime detection non-functional |
| H8 | No LLM in pipeline | Only orchestrator calls LLM. Pipeline binary has zero AI involvement |
| H9 | rat-agents processors disconnected | 28 processors with real logic — never called from pipeline |

## MEDIUM (architecture/quality)

| # | Issue | What's Wrong |
|---|-------|-------------|
| M1 | 326 unwrap() calls | Many in production code — will panic on unexpected data |
| M2 | 152 compiler warnings | Unused variables indicate dead code paths |
| M3 | rat-maintance typo | Crate name has misspelling |
| M4 | No Docker deployment | Freqtrade has docker-compose.yml — RAT has nothing |
| M5 | No Telegram integration | Freqtrade's killer feature — remote control from phone |
| M6 | No WebUI | Freqtrade has built-in web interface |
| M7 | No CI/CD | Freqtrade has GitHub Actions — RAT has nothing |
| M8 | No proper backtesting | Freqtrade has HyperOpt, lookahead analysis, recursive analysis. RAT has basic simulator |
| M9 | No pair locking | Freqtrade locks pairs after exit to prevent re-entry waterfalls |
| M10 | No wallet tracking in strategy | Freqtrade exposes wallet balances to strategy logic |
| M11 | No notification system | Freqtrade sends Telegram alerts on trade events |
| M12 | No plotting tools | Freqtrade has matplotlib charts for strategy visualization |
| M13 | No strategy repository | Freqtrade has community strategy library |

## LOW (polish)

| # | Issue | What's Wrong |
|---|-------|-------------|
| L1 | Live broker place_order → "Not implemented" | Binance/Zerodha clients have auth but no order execution |
| L2 | 5 dead code warnings in rat-tui | Command, CommandPalette, RiskDashboard structs unused |
| L3 | 106 warnings in rat-agents | Unused variables, non-camel-case types |
| L4 | No multi-account support | Single account only |
| L5 | No order book depth analysis | Only price/volume — no L2 data |
| L6 | No slippage model | Paper trades assume perfect fills |
| L7 | No commission modeling | Zero fees in paper mode |
| L8 | No trade journal persistence | JournalKeeper records to memory but no SQLite |

## Comparison: RAT vs Freqtrade

| Feature | RAT Agent | Freqtrade |
|---------|-----------|-----------|
| Language | Rust (fast) | Python (flexible) |
| Stars | — | 52k |
| Exchanges | 6 brokers | 10+ via ccxt |
| Backtesting | Basic | HyperOpt + ML |
| ML/AI | FreqAI equivalent | FreqAI built-in |
| WebUI | No | Yes |
| Telegram | No | Yes |
| Docker | No | Yes |
| Strategy repo | No | Yes |
| Risk rules | 29 hard rules | Configurable |
| Agent hierarchy | 21 agents | None |
| Memory system | Vector + graph | SQLite only |
| Regime detection | Built-in | Manual |
| Self-evolution | Built-in | Manual |
| Real-time feeds | 6 feed modules | DataProvider |
| Pattern detection | 5 detectors | Manual |
| Reasoning chains | 7 types | None |
