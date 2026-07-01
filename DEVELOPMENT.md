# RAT Agent — Development Guide

## Prerequisites

- Rust 1.96.0+
- Cargo (comes with Rust)
- Git

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build specific crate
cargo build -p rat-pipeline
```

## Running

```bash
# Run trading pipeline
./target/release/rat-pipeline --symbols BTC,ETH --mode paper

# Run TUI
./target/release/rat-tui

# Run with custom symbols
./target/release/rat-pipeline --symbols BTC,ETH,SOL,ADA
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test -p rat-agents
cargo test -p rat-pipeline

# Run specific test
cargo test test_risk_check
```

## Code Generation

```bash
# Add new agent
mkdir -p crates/rat-agents/src/agents/new_agent
# Create: new_agent.rs, new_agent_skills.rs, new_agent_rules.rs, new_agent_tools.rs, new_agent_memory.rs

# Add new indicator
mkdir -p crates/rat-indicators/src/new_indicator
# Create: new_indicator.rs, new_indicator_skills.rs, new_indicator_rules.rs, new_indicator_tools.rs

# Add new rule
mkdir -p crates/rat-rules/src/new_rule
# Create: new_rule.rs, new_rule_skills.rs, new_rule_rules.rs, new_rule_tools.rs
```

## Environment Variables

```bash
# Memory service
export MEMORY_API_URL=http://localhost:3111

# Broker API keys
export BINANCE_API_KEY=your_key
export BINANCE_API_SECRET=your_secret

# Trading mode
export TRADING_MODE=paper
```

## Architecture Patterns

- **Agent Pattern:** Each agent has `agent.rs` + `skills.rs` + `rules.rs` + `tools.rs` + `memory.rs`
- **Pipeline Pattern:** 5-phase flow: Data → Signal → Risk → Execute → Feedback
- **Event-Driven:** All inter-agent communication via tokio broadcast channels
- **Memory-First:** Every agent stores/retrieves via agentic-memory
