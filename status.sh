#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# RAT Agent — Service Status
# Shows running status and health of all three services.
# ═══════════════════════════════════════════════════════════════════════════
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/.env"

PIDDIR="$SCRIPT_DIR/logs"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

check_http_service() {
    local name="$1"
    local port="$2"
    local pidfile="$PIDDIR/$3.pid"
    local health_url="$4"

    local pid="—"
    local pid_status="${RED}stopped${NC}"
    local health="—"

    if [ -f "$pidfile" ]; then
        pid=$(cat "$pidfile")
        if kill -0 "$pid" 2>/dev/null; then
            pid_status="${GREEN}running${NC}"
        else
            pid_status="${YELLOW}dead (stale PID)${NC}"
        fi
    fi

    if curl -sf "$health_url" > /dev/null 2>&1; then
        health="${GREEN}healthy${NC}"
    elif nc -z localhost "$port" 2>/dev/null; then
        health="${YELLOW}port open, no health${NC}"
    else
        health="${RED}unreachable${NC}"
    fi

    printf "  %-22s  port %-6s  PID %-8s  %b  %b\n" "$name" "$port" "$pid" "$pid_status" "$health"
}

check_process_service() {
    local name="$1"
    local pidfile="$PIDDIR/$2.pid"
    local process_pattern="$3"

    local pid="—"
    local pid_status="${RED}stopped${NC}"

    if [ -f "$pidfile" ]; then
        pid=$(cat "$pidfile")
        if kill -0 "$pid" 2>/dev/null; then
            pid_status="${GREEN}running (PID $pid)${NC}"
        else
            pid_status="${YELLOW}dead (stale PID)${NC}"
        fi
    fi

    printf "  %-22s  %-27s  PID %-8s  %b\n" "$name" "(autonomous bot)" "$pid" "$pid_status"
}

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║            RAT Agent — Service Status                       ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""

printf "  %-22s  %-27s  PID %-8s  %-16s %b\n" "SERVICE" "ENDPOINT" "PID" "PROCESS" "HEALTH"
echo "  ─────────────────────────────────────────────────────────────────────────────────"

check_http_service "Agentic Memory" "$MEMORY_PORT" "memory" "http://localhost:$MEMORY_PORT/health"
check_http_service "Tredo Exchange" "$TREDO_PORT"  "tredo"  "http://localhost:$TREDO_PORT/api/v1/health"
check_process_service "RAT" "rat" ""

echo ""
echo "  Logs: $SCRIPT_DIR/logs/"
echo "  Start: ./start.sh    Stop: ./stop.sh"
