#!/usr/bin/env bash
#
# install_service.sh — Install and enable the rat orchestrator auto-start service
#
# Usage:
#   ./scripts/install_service.sh                  # auto-detect platform
#   ./scripts/install_service.sh --launchd         # force macOS launchd
#   ./scripts/install_service.sh --systemd         # force Linux systemd
#   ./scripts/install_service.sh --cron            # simple cron@reboot fallback
#   ./scripts/install_service.sh --uninstall       # remove the service
#   ./scripts/install_service.sh --status          # check if running
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

BINARY="$PROJECT_DIR/target/release/rat-orchestrator"
[ -f "$BINARY" ] || BINARY="$PROJECT_DIR/target/debug/rat-orchestrator"

# ── Helpers ─────────────────────────────────────────────────────────────────
info()  { printf "  \033[1;34m•\033[0m %s\n" "$*"; }
ok()    { printf "  \033[1;32m✓\033[0m %s\n" "$*"; }
warn()  { printf "  \033[1;33m⚠\033[0m %s\n" "$*" >&2; }
fail()  { printf "  \033[1;31m✗\033[0m %s\n" "$*" >&2; exit 1; }

# ── Pre-flight checks ───────────────────────────────────────────────────────
preflight() {
    if [ ! -f "$BINARY" ]; then
        fail "Binary not found. Build first:\n    cd $PROJECT_DIR && cargo build --release"
    fi
    if [ ! -f "$PROJECT_DIR/config/rat.env" ]; then
        fail "Config not found: $PROJECT_DIR/config/rat.env"
    fi
    chmod +x "$SCRIPT_DIR/start_orchestrator.sh"
    ok "Pre-flight checks passed"
}

# ── macOS launchd ────────────────────────────────────────────────────────────
install_launchd() {
    local plist_src="$SCRIPT_DIR/com.rat.orchestrator.plist"
    local plist_dst="$HOME/Library/LaunchAgents/com.rat.orchestrator.plist"

    # Update the plist paths to match the actual project location
    sed "s|/Users/varma/Desktop/Agentic application/Rat|$PROJECT_DIR|g" \
        "$plist_src" > "$plist_dst"
    chmod 644 "$plist_dst"

    launchctl unload "$plist_dst" 2>/dev/null || true
    launchctl load -w "$plist_dst"
    ok "launchd service installed and loaded"
}

uninstall_launchd() {
    local plist_dst="$HOME/Library/LaunchAgents/com.rat.orchestrator.plist"
    if [ -f "$plist_dst" ]; then
        launchctl unload "$plist_dst" 2>/dev/null || true
        rm "$plist_dst"
        ok "launchd service removed"
    else
        warn "No launchd plist found"
    fi
}

status_launchd() {
    local plist_dst="$HOME/Library/LaunchAgents/com.rat.orchestrator.plist"
    if [ -f "$plist_dst" ]; then
        launchctl list com.rat.orchestrator 2>/dev/null && \
            info "Service is loaded" || \
            warn "Service plist exists but not loaded"
    else
        warn "Service not installed"
    fi
}

# ── Linux systemd ────────────────────────────────────────────────────────────
install_systemd() {
    local unit_src="$SCRIPT_DIR/rat-orchestrator.service"
    local unit_dst="/etc/systemd/system/rat-orchestrator.service"

    sudo mkdir -p /opt/rat
    sudo ln -sf "$PROJECT_DIR" /opt/rat 2>/dev/null || \
        sudo cp -r "$PROJECT_DIR"/* /opt/rat/

    sudo cp "$unit_src" "$unit_dst"
    sudo systemctl daemon-reload
    sudo systemctl enable rat-orchestrator
    sudo systemctl restart rat-orchestrator
    ok "systemd service installed and started"
}

uninstall_systemd() {
    sudo systemctl stop rat-orchestrator 2>/dev/null || true
    sudo systemctl disable rat-orchestrator 2>/dev/null || true
    sudo rm -f /etc/systemd/system/rat-orchestrator.service
    sudo systemctl daemon-reload
    ok "systemd service removed"
}

status_systemd() {
    if systemctl is-enabled rat-orchestrator 2>/dev/null | grep -q enabled; then
        info "Service is enabled"
        systemctl --no-pager status rat-orchestrator 2>/dev/null | head -10
    else
        warn "Service not installed or not enabled"
    fi
}

# ── Cron @reboot fallback ────────────────────────────────────────────────────
install_cron() {
    local cron_cmd="@reboot cd $PROJECT_DIR && export LOG_DIR=/tmp/rat-logs && $SCRIPT_DIR/start_orchestrator.sh # rat-orchestrator"
    (crontab -l 2>/dev/null | grep -v 'rat-orchestrator' || true; echo "$cron_cmd") | crontab -
    ok "cron @reboot entry added"
}

uninstall_cron() {
    crontab -l 2>/dev/null | grep -v 'rat-orchestrator' | crontab - || true
    ok "cron @reboot entry removed"
}

status_cron() {
    if crontab -l 2>/dev/null | grep -q 'rat-orchestrator'; then
        info "cron @reboot entry exists"
    else
        warn "No cron @reboot entry for rat"
    fi
}

# ── Platform detection ───────────────────────────────────────────────────────
detect_platform() {
    case "$(uname -s)" in
        Darwin*) echo "launchd" ;;
        Linux*)  echo "systemd" ;;
        *)       echo "cron" ;;
    esac
}

# ── Main ──────────────────────────────────────────────────────────────────────
main() {
    local platform
    local action="install"

    case "${1:-}" in
        --launchd)   platform="launchd" ;;
        --systemd)   platform="systemd" ;;
        --cron)      platform="cron" ;;
        --uninstall) action="uninstall"; platform="$(detect_platform)" ;;
        --status)    action="status";    platform="$(detect_platform)" ;;
        --help|-h)
            echo "Usage: $0 [--launchd|--systemd|--cron|--uninstall|--status]"
            exit 0
            ;;
        *) platform="$(detect_platform)" ;;
    esac

    echo ""
    echo "  ╔══════════════════════════════════════════╗"
    echo "  ║   rat — Service Installer              ║"
    printf "  ║   Platform: %-30s ║\n" "$platform"
    printf "  ║   Action:   %-30s ║\n" "$action"
    echo "  ╚══════════════════════════════════════════╝"
    echo ""

    if [ "$action" != "status" ]; then
        preflight
    fi

    case "$platform" in
        launchd)
            case "$action" in
                install)   install_launchd ;;
                uninstall) uninstall_launchd ;;
                status)    status_launchd ;;
            esac
            ;;
        systemd)
            case "$action" in
                install)   install_systemd ;;
                uninstall) uninstall_systemd ;;
                status)    status_systemd ;;
            esac
            ;;
        cron)
            case "$action" in
                install)   install_cron ;;
                uninstall) uninstall_cron ;;
                status)    status_cron ;;
            esac
            ;;
        *)
            fail "Unsupported platform: $(uname -s). Use --cron for a basic @reboot entry."
            ;;
    esac

    echo ""
    ok "Done!"
    echo ""
}

main "$@"
