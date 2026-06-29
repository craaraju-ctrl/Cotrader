#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PIDDIR="$SCRIPT_DIR/logs"

CYAN='\033[0;36m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
log()  { echo -e "${CYAN}[STOP]${NC} $1"; }
ok()   { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

stop_service() {
    local name="$1" pidfile="$PIDDIR/$2.pid"
    [ ! -f "$pidfile" ] && { warn "$name: not running"; return; }
    local pid; pid=$(cat "$pidfile")
    kill -0 "$pid" 2>/dev/null || { warn "$name: stale PID"; rm -f "$pidfile"; return; }
    log "Stopping $name (PID $pid)..."
    kill "$pid" 2>/dev/null
    local i=0; while kill -0 "$pid" 2>/dev/null && [ $i -lt 10 ]; do sleep 1; i=$((i+1)); done
    kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null
    rm -f "$pidfile"; ok "$name stopped"
}

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║            RAT Agent — Stopping Services                    ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""

stop_service "RAT"            "rat"
stop_service "Tredo Exchange" "tredo"
stop_service "Agentic Memory" "memory"

echo -e "\n${GREEN}All services stopped.${NC}"
