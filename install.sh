#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# RAT Agent — One-Line Installer
#
# Install from GitHub:
#   curl -fsSL https://raw.githubusercontent.com/craaraju-ctrl/rat-agent/main/install.sh | bash
#
# Or clone manually:
#   git clone https://github.com/craaraju-ctrl/rat-agent.git
#   cd rat-agent && bash install.sh
# ═══════════════════════════════════════════════════════════════════════════════
set -euo pipefail

# ── Colors ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $1"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
err()   { echo -e "${RED}[ERR]${NC}   $1"; exit 1; }

# ── Config ───────────────────────────────────────────────────────────────────
REPO_URL="https://github.com/craaraju-ctrl/rat-agent.git"
INSTALL_DIR="${RAT_INSTALL_DIR:-$HOME/.rat-agent}"
REQUIRED_RUST="1.75.0"  # Minimum Rust version (2024 edition needs 1.85+)
BIN_DIR="${HOME}/.local/bin"

# ── Banner ───────────────────────────────────────────────────────────────────
echo ""
echo -e "${CYAN}${BOLD}"
echo "  ╔═══════════════════════════════════════════════════════════╗"
echo "  ║                                                           ║"
echo "  ║      RAT Agent — Autonomous Trading System                ║"
echo "  ║      Trading Real-time Edge Decision Optimisation         ║"
echo "  ║                                                           ║"
echo "  ╚═══════════════════════════════════════════════════════════╝"
echo -e "${NC}"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# Step 1: Check / Install Prerequisites
# ═══════════════════════════════════════════════════════════════════════════════

info "Checking prerequisites..."

# Git
if ! command -v git &>/dev/null; then
    info "Installing git..."
    if [[ "$OSTYPE" == "darwin"* ]]; then
        xcode-select --install 2>/dev/null || true
    elif command -v apt-get &>/dev/null; then
        sudo apt-get update && sudo apt-get install -y git
    elif command -v dnf &>/dev/null; then
        sudo dnf install -y git
    elif command -v pacman &>/dev/null; then
        sudo pacman -S --noconfirm git
    else
        err "Git not found. Install git manually."
    fi
fi
ok "Git $(git --version | awk '{print $3}')"

# Rust
if ! command -v rustc &>/dev/null; then
    info "Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
    ok "Rust installed"
else
    RUST_VERSION=$(rustc --version | awk '{print $2}')
    ok "Rust $RUST_VERSION"
fi

# Ensure cargo is on PATH
export PATH="$HOME/.cargo/bin:$PATH"

# ═══════════════════════════════════════════════════════════════════════════════
# Step 2: Clone or Update Repository
# ═══════════════════════════════════════════════════════════════════════════════

if [ -d "$INSTALL_DIR/.git" ]; then
    info "Repository exists at $INSTALL_DIR — updating..."
    cd "$INSTALL_DIR"
    git pull --rebase origin main 2>/dev/null || {
        warn "Pull failed — using existing code"
    }
else
    info "Cloning RAT Agent to $INSTALL_DIR..."
    rm -rf "$INSTALL_DIR"
    git clone --depth 1 "$REPO_URL" "$INSTALL_DIR"
    cd "$INSTALL_DIR"
fi
ok "Source code ready at $INSTALL_DIR"

# ═══════════════════════════════════════════════════════════════════════════════
# Step 3: Build Release Binaries
# ═══════════════════════════════════════════════════════════════════════════════

info "Building release binaries (this may take 5-10 minutes on first install)..."
echo ""

cargo build --release 2>&1 | tail -3

if [ ! -f "target/release/rat" ]; then
    err "Build failed — target/release/rat not found"
fi
ok "Build complete"

# ═══════════════════════════════════════════════════════════════════════════════
# Step 4: Install Binaries to PATH
# ═══════════════════════════════════════════════════════════════════════════════

mkdir -p "$BIN_DIR"

# Symlink all RAT binaries into ~/.local/bin
for bin in rat rat-pipeline rat-tui rat-cli rat-orchestrator; do
    if [ -f "target/release/$bin" ]; then
        ln -sf "$INSTALL_DIR/target/release/$bin" "$BIN_DIR/$bin"
        ok "Installed: $bin -> $BIN_DIR/$bin"
    fi
done

# Install launcher script
ln -sf "$INSTALL_DIR/install.sh" "$BIN_DIR/rat-install"
ok "Installed: rat-install -> $BIN_DIR/rat-install"

# ═══════════════════════════════════════════════════════════════════════════════
# Step 5: Create default .env if missing
# ═══════════════════════════════════════════════════════════════════════════════

if [ ! -f "$INSTALL_DIR/.env" ]; then
    cat > "$INSTALL_DIR/.env" << 'ENVEOF'
# RAT Agent — Environment Configuration
# ═══════════════════════════════════════════════════════════════

# Memory Service
MEMORY_PORT=3111
MEMORY_DB_PATH=memory/data.db

# Tredo Exchange
TREDO_PORT=8080

# RAT Bot
RAT_PORT=8082
WEB_API_ADDR=0.0.0.0:8082
PAPER_MODE=true
INITIAL_BALANCE=100000
WS_ENABLED=true
WATCHLIST=BTC,ETH,SOL

# Ollama (optional — for vector embeddings)
OLLAMA_BASE_URL=http://localhost:11434
OLLAMA_MODEL=nomic-embed-text

# LLM (optional — stubbed in current version)
LLM_PROVIDER=ollama
LLM_MODEL=codellama
LLM_ENDPOINT=http://localhost:11434

# Binance API (for live trading — leave blank for paper mode)
BINANCE_API_KEY=
BINANCE_API_SECRET=

# Logging
RUST_LOG=info

# Memory API URL
MEMORY_API_URL=http://localhost:3111
ENVEOF
    ok "Created default .env"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 6: Ensure ~/.local/bin is on PATH
# ═══════════════════════════════════════════════════════════════════════════════

SHELL_RC=""
if [ -f "$HOME/.zshrc" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_RC="$HOME/.bashrc"
elif [ -f "$HOME/.bash_profile" ]; then
    SHELL_RC="$HOME/.bash_profile"
fi

if [ -n "$SHELL_RC" ]; then
    if ! grep -q '$HOME/.local/bin' "$SHELL_RC" 2>/dev/null; then
        echo '' >> "$SHELL_RC"
        echo '# RAT Agent — add to PATH' >> "$SHELL_RC"
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_RC"
        ok "Added $BIN_DIR to PATH in $SHELL_RC"
        warn "Run: source $SHELL_RC  (or open a new terminal)"
    else
        ok "$BIN_DIR already on PATH"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Done
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${GREEN}${BOLD}"
echo "  ╔═══════════════════════════════════════════════════════════╗"
echo "  ║             RAT Agent — Installation Complete!            ║"
echo "  ╚═══════════════════════════════════════════════════════════╝"
echo -e "${NC}"
echo ""
echo "  Quick Start:"
echo ""
echo -e "    ${CYAN}rat start${NC}              Start all services (memory + pipeline)"
echo -e "    ${CYAN}rat start --tui${NC}        Start with terminal UI"
echo -e "    ${CYAN}rat start --mode live${NC}  Start in live trading mode"
echo ""
echo "  Commands:"
echo ""
echo -e "    ${CYAN}rat start${NC}              Launch all services"
echo -e "    ${CYAN}rat stop${NC}               Stop all services"
echo -e "    ${CYAN}rat status${NC}             Check service status"
echo -e "    ${CYAN}rat serve${NC}              Run trading directly"
echo -e "    ${CYAN}rat list${NC}               List available brokers"
echo -e "    ${CYAN}rat configure <id>${NC}     Configure a broker"
echo ""
echo "  Install dir:  $INSTALL_DIR"
echo "  Binaries:     $BIN_DIR"
echo "  Config:       $INSTALL_DIR/.env"
echo ""
echo -e "  ${YELLOW}New terminal required if PATH was just updated.${NC}"
echo ""
