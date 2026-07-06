#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# RAT Agent — Start All Services
# ═══════════════════════════════════════════════════════════════════════════
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/.env"

LOGDIR="$SCRIPT_DIR/logs"
PIDDIR="$SCRIPT_DIR/logs"
mkdir -p "$LOGDIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { echo -e "${CYAN}[START]${NC} $1"; }
ok()   { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

wait_for_port() {
    local port=$1 name=$2 max_wait=${3:-30} i=0
    while ! nc -z localhost "$port" 2>/dev/null; do
        i=$((i + 1))
        [ $i -ge $max_wait ] && { warn "$name timeout on port $port"; return 1; }
        sleep 1
    done
    ok "$name listening on port $port"
}

is_running() {
    [ -f "$1" ] && kill -0 "$(cat "$1")" 2>/dev/null
}

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║            RAT Agent — Starting Services                    ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""

cd "$SCRIPT_DIR"

# ── 1. Agentic Memory ──────────────────────────────────────────────────
PIDFILE_MEM="$PIDDIR/memory.pid"
if is_running "$PIDFILE_MEM"; then
    warn "Agentic Memory already running (PID $(cat "$PIDFILE_MEM"))"
else
    log "Starting Agentic Memory on port $MEMORY_PORT..."
    MEMORY_DB_PATH="$MEMORY_DB_PATH" \
    OLLAMA_BASE_URL="$OLLAMA_BASE_URL" \
    OLLAMA_MODEL="$OLLAMA_MODEL" \
    ./target/release/agentic-memory > "$LOGDIR/memory.log" 2>&1 &
    echo $! > "$PIDFILE_MEM"
    wait_for_port "$MEMORY_PORT" "Agentic Memory" 45
fi

# ── 2. RAT ─────────────────────────────────────────────────────────────
PIDFILE_RAT="$PIDDIR/rat.pid"
if is_running "$PIDFILE_RAT"; then
    warn "RAT already running (PID $(cat "$PIDFILE_RAT"))"
else
    log "Starting RAT autonomous trading bot..."
    PORT="$RAT_PORT" \
    WEB_API_ADDR="$WEB_API_ADDR" \
    LLM_PROVIDER="$LLM_PROVIDER" \
    LLM_MODEL="$LLM_MODEL" \
    LLM_ENDPOINT="$LLM_ENDPOINT" \
    OLLAMA_BASE_URL="$OLLAMA_BASE_URL" \
    OLLAMA_MODEL="$OLLAMA_MODEL" \
    PAPER_MODE="$PAPER_MODE" \
    INITIAL_BALANCE="$INITIAL_BALANCE" \
    WS_ENABLED="$WS_ENABLED" \
    WATCHLIST="$WATCHLIST" \
    MEMORY_API_URL="$MEMORY_API_URL" \
    RUST_LOG=info \
    ./target/release/cotrader-orchestrator > "$LOGDIR/rat.log" 2>&1 &
    echo $! > "$PIDFILE_RAT"
    sleep 5
    if kill -0 "$(cat "$PIDFILE_RAT")" 2>/dev/null; then
        ok "RAT started (PID $(cat "$PIDFILE_RAT"))"
    else
        warn "RAT may have failed — check logs/rat.log"
    fi
fi

echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║              All services started!                          ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "  Agentic Memory : http://localhost:$MEMORY_PORT  (PID $(cat "$PIDFILE_MEM"))"
echo "  RAT            : autonomous trading bot          (PID $(cat "$PIDFILE_RAT"))"
echo ""
echo "  Logs: $LOGDIR/"
echo "  Stop: ./stop.sh"
