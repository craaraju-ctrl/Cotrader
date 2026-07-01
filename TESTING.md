# RAT Agent — Testing Guide

## Test Structure

```
crates/
├── rat-autonomous/tests/     Integration tests
├── rat-core/tests/           Core tests
├── rat-agents/               Unit tests per agent
└── rat-pipeline/             Pipeline tests
```

## Running Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p rat-autonomous

# Specific test
cargo test test_risk_check

# With output
cargo test -- --nocapture
```

## Test Categories

| Category | Count | Purpose |
|----------|-------|---------|
| Unit tests | 50+ | Individual function testing |
| Integration tests | 30+ | Component interaction |
| Pipeline tests | 20+ | End-to-end flow |
| Agent tests | 12+ | Agent behavior |

## Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_risk_check() {
        let engine = Engine::new(EngineConfig::default());
        let result = engine.check_risk("BTC", 0.001, 50000.0).await;
        assert!(result.passed);
    }
}
```

## CI/CD

Tests run automatically on:
- Pull requests
- Main branch merges
- Nightly builds
