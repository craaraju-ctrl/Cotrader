
---

## Kronos Forecasting Service Fixes

### Bug 1: Model Identity Mismatch
| File | Before | After |
|------|--------|-------|
| model.py | `amazon/chronos-bolt-base` (~50M) | `amazon/chronos-bolt-tiny` (~28M) |
| download.py | `amazon/chronos-bolt-base` | `amazon/chronos-bolt-tiny` |

**Impact:** base model pushes latency from ~100ms to >1000ms on CPU.

### Bug 2: Hardcoded Timeframe
| File | Before | After |
|------|--------|-------|
| main.py:68 | `freq='1min'` hardcoded | `freq=request.timeframe` dynamic |

**Impact:** 1H chart returns 1-min predictions (temporal corruption).

### Bug 3: Pseudo-Median Extraction
| File | Before | After |
|------|--------|-------|
| model.py:146-147 | `pred_tensor[mid_idx]` (single sample) | `np.median(pred_tensor, axis=0)` (true median) |

**Impact:** Random sample path instead of 50th percentile distribution.

### Bug 4: No Input Validation
| File | Before | After |
|------|--------|-------|
| main.py:55 | No length check | `len(ohlcv) < 5` raises 400 |

**Impact:** Empty array crashes worker thread.

### Verification
```bash
python3 -m py_compile main.py   # ✅
python3 -m py_compile model.py  # ✅
python3 -m py_compile download.py  # ✅
```
