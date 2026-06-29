#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# RAT Agent — Build All
# Single workspace build for the unified project.
# ═══════════════════════════════════════════════════════════════════════════
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
NC='\033[0m'

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║          RAT Agent — Building Workspace                     ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""

cargo build --release 2>&1

echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║              Build complete!                                ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════════════╝${NC}"
