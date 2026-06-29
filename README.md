# RAT Agent — Realtime Autonomous Trading Agent

Unified launcher that runs **Tredo Exchange**, **RAT**, and **Agentic Memory** together as one system.

```
/Users/varma/Desktop/Trading/RAT Agent/
├── .env                    # Shared config (ports, paths, URLs)
├── build.sh                # Build all three projects
├── start.sh                # Launch everything
├── stop.sh                 # Shutdown everything
├── status.sh               # Health check dashboard
├── README.md               # This file
├── logs/                   # Runtime logs
├── data/                   # Persistent storage (Memory SQLite)
├── Tredo Exchange/         # Matching engine (port 8080)
├── rat/                    # Autonomous trading brain (port 8082)
└── Agentic Memory/         # Shared memory server (port 3111)
```

## Prerequisites

| Dependency | Required | Install |
|---|---|---|
| Rust 1.75+ | Yes | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| PostgreSQL | Yes (Tredo) | `brew install postgresql@16 && brew services start postgresql@16` |
| Ollama | Yes (RAT + Memory) | `curl -fsSL https://ollama.com/install.sh \| sh` |

### Setup

```bash
# Create database
createdb tredo_exchange

# Pull models
ollama pull nemotron-3-nano:4b
ollama pull nomic-embed-text
```

## Quick Start

```bash
cd "/Users/varma/Desktop/Trading/RAT Agent"

./build.sh      # build all three
./start.sh      # launch everything
./status.sh     # check health
./stop.sh       # shut down
```

## Port Map

| Service | Port | URL |
|---|---|---|
| Tredo Exchange | 8080 | http://localhost:8080 |
| RAT | 8082 | http://localhost:8082 |
| Agentic Memory | 3111 | http://localhost:3111 |
| Ollama | 11434 | http://localhost:11434 |
