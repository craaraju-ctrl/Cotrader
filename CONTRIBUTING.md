# Contributing to RAT Agent

## Development Setup

```bash
# Clone the repository
git clone https://github.com/your-repo/rat-agent.git
cd rat-agent

# Install Rust toolchain
rustup default 1.96.0

# Build the project
cargo build --release

# Run tests
cargo test --workspace
```

## Project Structure

```
crates/
├── rat-core/          Core types and memory
├── rat-autonomous/    Pipeline and agents
├── rat-agents/        21 trading agents
├── rat-pipeline/      Pipeline orchestration
├── rat-brokers/       Multi-broker abstraction
├── rat-indicators/    Technical indicators
├── rat-rules/         Risk rules
├── rat-strategies/    Trading strategies
├── rat-risk/          Risk components
├── rat-skills/        Agent skills
├── rat-tui/           Terminal UI
└── rat-memory/        Agentic memory
```

## Code Style

- Use `cargo fmt` before committing
- Run `cargo clippy` to catch issues
- Add tests for new functionality
- Document public APIs with `///` comments

## Pull Request Process

1. Create a feature branch
2. Make changes with tests
3. Run `cargo test --workspace`
4. Submit PR with description of changes
