#!/usr/bin/env bash
# RAT Agent — Uninstall
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

INSTALL_DIR="${RAT_INSTALL_DIR:-$HOME/.cotrader-agent}"
BIN_DIR="${HOME}/.local/bin"

echo ""
echo -e "${CYAN}RAT Agent — Uninstall${NC}"
echo ""

# Stop services first
if [ -f "$INSTALL_DIR/target/release/rat" ]; then
    "$INSTALL_DIR/target/release/rat" stop 2>/dev/null || true
fi

# Remove binaries
for bin in rat cotrader-pipeline cotrader-tui cotrader-cli cotrader-orchestrator cotrader-install; do
    if [ -L "$BIN_DIR/$bin" ]; then
        rm -f "$BIN_DIR/$bin"
        echo -e "  ${GREEN}Removed${NC} $BIN_DIR/$bin"
    fi
done

# Remove install directory
if [ -d "$INSTALL_DIR" ]; then
    echo ""
    read -p "  Delete $INSTALL_DIR (all data, logs, databases)? [y/N] " confirm
    if [[ "$confirm" =~ ^[Yy]$ ]]; then
        rm -rf "$INSTALL_DIR"
        echo -e "  ${GREEN}Removed${NC} $INSTALL_DIR"
    else
        echo "  Kept $INSTALL_DIR"
    fi
fi

echo ""
echo -e "${GREEN}RAT Agent uninstalled.${NC}"
echo ""
