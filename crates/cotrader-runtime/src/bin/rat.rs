//! cotrader — Autonomous Trading System
//!
//! Usage:
//!     cotrader                              # Run paper trading (default)
//!     cotrader serve                        # Run paper trading (alias)
//!     cotrader serve --mode live --confirm-live  # Live trading (requires confirmation)
//!     cotrader serve --mode backtest --data ./data.csv --capital 100000
//!     cotrader serve --mode validate --cycles 100
//!     cotrader serve --mode research         # Observe market, no trading
//!     cotrader start                         # Start exchange + trading services
//!     cotrader start --mode live             # Start with live trading mode
//!     cotrader start --symbols BTC,ETH,SOL   # Start with custom symbols
//!     cotrader stop                          # Stop all running services
//!     cotrader status                        # Show all service health
//!     cotrader build                         # Build the project
//!     cotrader start-kronos                  # Start Kronos forecast server
//!     cotrader start-kronos --port 8000      # Start Kronos on custom port
//!     cotrader download                      # Download Chronos model
//!     cotrader list                          # List available brokers
//!     cotrader configure <broker_id>         # Configure a broker
//!     cotrader cache                         # Show policy cache stats
//!
//! All modes share the same agent core, so backtested strategies = live strategies.

use anyhow::Context;
use clap::{Parser, Subcommand};
use colored::*;
use std::sync::Arc;
use cotrader_autonomous::AutonomousOrchestrator;
use cotrader_core::paper_engine::{BrokerAdapter, BrokerRegistry};
use cotrader_runtime::broker::{BrokerConfig, BrokerPluginManager};
use cotrader_runtime::engine::RuntimeEngine;
use cotrader_runtime::mode::{ModeConfig, TradingMode};

#[derive(Parser, Debug)]
#[command(
    name = "cotrader",
    version,
    about = "cotrader — Autonomous Trading System",
    long_about = "cotrader: Autonomous Trading System\n\nA production-grade, Rust-first autonomous agentic trading co-pilot.\n\nUse `cotrader serve` to run paper/live trading.\nUse `cotrader start` to launch the full pipeline."
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the trading pipeline
    Start {
        /// Trading mode: paper, live
        #[arg(long, default_value_t = TradingMode::Paper)]
        mode: TradingMode,

        /// Symbols to trade (comma-separated)
        #[arg(long, default_value_t = String::from("BTC,ETH"))]
        symbols: String,

        /// REQUIRED for live mode: explicit confirmation
        #[arg(long, default_value_t = false)]
        confirm_live: bool,

        /// Launch the terminal UI after services start
        #[arg(long, default_value_t = false)]
        tui: bool,
    },

    /// Stop all running services
    Stop,

    /// Show status of all services
    Status,

    /// Build the project in release mode
    Build {
        /// Specific package to build
        #[arg(long)]
        package: Option<String>,
    },

    /// Run the trading system directly (no service orchestration)
    Serve {
        /// Trading mode: paper, live, backtest, validate, research
        #[arg(long, default_value_t = TradingMode::Paper)]
        mode: TradingMode,

        /// REQUIRED for live mode: explicit confirmation
        #[arg(long, default_value_t = false)]
        confirm_live: bool,

        /// Required for backtest mode: path to CSV data file
        #[arg(long)]
        data: Option<String>,

        /// Number of cycles for validate mode
        #[arg(long, default_value_t = 50)]
        cycles: usize,

        /// Induce regret to force rule adaptation (validate mode)
        #[arg(long, default_value_t = false)]
        induce_regret: bool,

        /// Max daily loss in currency (default 1000)
        #[arg(long, default_value_t = 1000.0)]
        max_daily_loss: f64,

        /// Starting capital for backtest (default 100000)
        #[arg(long, default_value_t = 100_000.0)]
        capital: f64,
    },

    /// List available brokers and their config schemas
    List,
    /// Configure a broker interactively (e.g., `rat configure zerodha`)
    Configure {
        /// Broker ID (e.g., "zerodha", "alpaca", "paper")
        broker_id: String,
    },
    /// Show policy cache health and top performers
    Cache,

    /// Start only the Kronos forecast server
    StartKronos {
        /// Port to run the server on (default: 8000)
        #[arg(long, default_value_t = 8000)]
        port: u16,
    },

    /// Download the Chronos-Bolt model from HuggingFace Hub
    Download,

    /// Generate shell completions
    Completions {
        /// Shell to generate for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helper: find the project root directory containing kronos_service/
// ═══════════════════════════════════════════════════════════════════════════════

fn find_project_root() -> std::path::PathBuf {
    // First, try CARGO_MANIFEST_DIR (set by cargo build/run)
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = std::path::PathBuf::from(&manifest);
        // CARGO_MANIFEST_DIR is .../crates/cotrader-runtime; project root is ../..
        let root = p.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        if let Some(root) = root {
            if root.join("kronos_service").join("main.py").exists() {
                return root;
            }
        }
    }

    // Try current working directory
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.join("kronos_service").join("main.py").exists() {
        return cwd;
    }

    // Try parent of cwd
    if let Some(parent) = cwd.parent() {
        if parent.join("kronos_service").join("main.py").exists() {
            return parent.to_path_buf();
        }
    }

    // Fallback: return cwd
    eprintln!("[cotrader] Could not locate kronos_service/ directory. Using current directory.");
    cwd
}

// ═══════════════════════════════════════════════════════════════════════════════
// Handler: rat start — unified launcher for all services
// ═══════════════════════════════════════════════════════════════════════════════

fn pid_dir() -> std::path::PathBuf {
    let root = find_project_root();
    let dir = root.join("logs");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn is_running(pid_file: &std::path::Path) -> bool {
    if !pid_file.exists() {
        return false;
    }
    let pid_str = std::fs::read_to_string(pid_file).unwrap_or_default();
    let pid: i32 = pid_str.trim().parse().unwrap_or(0);
    // kill -0 checks if process exists without sending a signal
    unsafe { libc::kill(pid, 0) == 0 }
}

fn write_pid(pid_file: &std::path::Path, pid: u32) {
    std::fs::write(pid_file, pid.to_string()).ok();
}

fn remove_pid(pid_file: &std::path::Path) {
    std::fs::remove_file(pid_file).ok();
}

async fn wait_for_port(port: u16, name: &str, max_secs: u64) -> bool {
    for i in 0..max_secs {
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .is_ok()
        {
            println!("  {} {} listening on port {}", "✓".green().bold(), name.green(), port);
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if i % 5 == 4 {
            println!("  {} Waiting for {} on port {} ({}s)...", "..".dimmed(), name, port, i + 1);
        }
    }
    println!("  {} {} did not respond on port {} after {}s", "⚠".yellow(), name.yellow(), port, max_secs);
    false
}

async fn handle_start_all(
    mode: TradingMode,
    symbols: String,
    _launch_tui: bool,
    _confirm_live: bool,
) -> anyhow::Result<()> {
    let root = find_project_root();
    let logs = pid_dir();

    println!();
    println!("  {} Starting Trading Pipeline", "CoTrader —".cyan().bold());
    println!("  Mode: {}  |  Symbols: {}", mode.to_string().yellow().bold(), symbols);
    println!();

    // Track child processes for graceful shutdown
    let mut children: Vec<(String, tokio::process::Child)> = Vec::new();

    // ── Trading Pipeline ─────────────────────────────────────────────
    println!("  [..] Starting Trading Pipeline ({}, symbols: {})...", mode, symbols);
    let bin_path = root.join("target").join("release").join("cotrader");
    if bin_path.exists() {
        let log_file = logs.join("pipeline.log");
        let mode_str = mode.to_string().to_lowercase();
        let child = tokio::process::Command::new(&bin_path)
            .args(["serve", "--mode", &mode_str])
            .current_dir(&root)
            .stdout(std::fs::File::create(&log_file).unwrap())
            .stderr(std::fs::File::create(logs.join("pipeline.err")).unwrap())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn trading pipeline")?;
        let pipeline_pid = logs.join("pipeline.pid");
        write_pid(&pipeline_pid, child.id().unwrap_or(0));
        children.push(("pipeline".into(), child));
        println!("  [OK] Trading pipeline started ({})", mode);
    } else {
        println!("  [WARN] cotrader binary not found at {} — pipeline not started", bin_path.display());
        println!("  [HINT] Run: cotrader build, then cotrader start");
    }

    println!();
    println!("  {}", "Pipeline started. Use the following commands:".green());
    println!("    {} — Check service health", "cotrader status".dimmed());
    println!("    {} — Stop all services", "cotrader stop".dimmed());
    println!("  Logs: {}/", logs.display());
    println!();

    // ── Wait for Ctrl+C ────────────────────────────────────────────────────
    println!("  Press Ctrl+C to stop the pipeline.");
    let _ = tokio::signal::ctrl_c().await;
    println!();
    println!("  Shutting down...");

    // Send SIGTERM to all children
    for (name, child) in &mut children {
        println!("  Stopping {}...", name);
        child.kill().await.ok();
    }

    println!("  Pipeline stopped.");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Handler: rat stop — stop all running services
// ═══════════════════════════════════════════════════════════════════════════════

/// Check if a service binary exists in the release directory.
fn binary_exists(name: &str) -> bool {
    let root = find_project_root();
    root.join("target").join("release").join(name).exists()
}

async fn handle_stop() -> anyhow::Result<()> {
    let logs = pid_dir();

    // Clean up any stale PID files from services that no longer exist
    for entry in std::fs::read_dir(&logs).ok().into_iter().flatten() {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pid") {
                // Check if this PID's binary exists; if not, remove stale file
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let bin_name = match name {
                    "pipeline" => "cotrader",
                    _ => continue, // unknown service, leave it alone
                };
                if !binary_exists(bin_name) {
                    // Binary doesn't exist — this is a stale PID from old version
                    if is_running(&path) {
                        let pid_str = std::fs::read_to_string(&path).unwrap_or_default();
                        let pid: i32 = pid_str.trim().parse().unwrap_or(0);
                        unsafe { libc::kill(pid, libc::SIGTERM) };
                    }
                    remove_pid(&path);
                }
            }
        }
    }

    // Only stop services whose binaries exist
    let services = [
        ("pipeline    ", logs.join("pipeline.pid"), "cotrader"),
    ];

    println!();
    for (name, pid_file, bin) in &services {
        if !binary_exists(bin) {
            continue;
        }
        if is_running(pid_file) {
            let pid_str = std::fs::read_to_string(pid_file).unwrap_or_default();
            let pid: i32 = pid_str.trim().parse().unwrap_or(0);
            unsafe { libc::kill(pid, libc::SIGTERM) };
            println!("  Stopped {} (PID {})", name, pid);
            remove_pid(pid_file);
        }
    }
    println!("  Done.");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Handler: rat status — show running services
// ═══════════════════════════════════════════════════════════════════════════════

async fn handle_status() -> anyhow::Result<()> {
    let logs = pid_dir();

    // Only show services whose binaries actually exist
    let mut services: Vec<(&str, std::path::PathBuf, u16)> = Vec::new();

    if binary_exists("cotrader") {
        services.push(("Trading Pipeline   ", logs.join("pipeline.pid"), 0));
    }
    if binary_exists("cotrader-tui") {
        services.push(("TUI Dashboard      ", logs.join("tui.pid"), 0));
    }

    println!();
    println!("  {} Service Status", "CoTrader —".cyan().bold());
    println!("  {}", "────────────────────────────────".dimmed());
    if services.is_empty() {
        println!("  {}", "No binaries found. Build with: cotrader build".yellow());
    } else {
        for (name, pid_file, port) in &services {
            let (status, pid_display) = if is_running(pid_file) {
                let pid = std::fs::read_to_string(pid_file).unwrap_or_default().trim().to_string();
                let port_info = if *port > 0 {
                    let port_open = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                        .await
                        .is_ok();
                    if port_open {
                        format!(" (port {}:HEALTHY)", port).green().to_string()
                    } else {
                        format!(" (port {}:UNREACHABLE)", port).red().to_string()
                    }
                } else {
                    String::new()
                };
                (format!("RUNNING{}", port_info).green().to_string(), format!("PID {}", pid).dimmed().to_string())
            } else {
                ("STOPPED".red().to_string(), String::new())
            };
            println!("  {} : {} {}", name, status, pid_display);
        }
    }
    println!();
    Ok(())
}

/// Attempt to find a working `python3` or `python` executable.
fn find_python() -> Option<String> {
    for candidate in &["python3", "python"] {
        if std::process::Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Some(candidate.to_string());
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Handler: rat start-kronos
// ═══════════════════════════════════════════════════════════════════════════════

async fn handle_start_kronos(port: u16) -> anyhow::Result<()> {
    let python = find_python().ok_or_else(|| {
        anyhow::anyhow!("Python not found. Install Python 3 and ensure `python3` is on your PATH.")
    })?;

    let root = find_project_root();
    let script = root.join("kronos_service").join("main.py");
    if !script.exists() {
        anyhow::bail!(
            "Kronos service script not found at: {}\n\
             Make sure you're running from the rat project root.",
            script.display()
        );
    }

    // Check if requirements are installed
    println!("[cotrader] 🐍 Using: {} from: {}", python, script.display());
    println!(
        "[cotrader] 🌐 Starting Kronos forecast server on port {}...",
        port
    );
    println!("[cotrader]    To stop: Ctrl+C\n");

    let mut restart_delay = std::time::Duration::from_millis(500);
    const MAX_RESTART_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

    loop {
        let mut child = tokio::process::Command::new(&python)
            .arg(&script)
            .env("KRONOS_PORT", port.to_string())
            .current_dir(&root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn python3 kronos_service/main.py")?;

        let pid = child.id().unwrap_or(0);
        println!("[cotrader] Kronos PID {} started", pid);

        // Spawn stdout reader
        let stdout = child.stdout.take();
        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    println!("[cotrader] {}", line);
                }
            });
        }

        // Spawn stderr reader
        let stderr = child.stderr.take();
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("[cotrader:err] {}", line);
                }
            });
        }

        // Wait for the child to exit OR for Ctrl+C
        let status = tokio::select! {
            status = child.wait() => status?,
            _ = tokio::signal::ctrl_c() => {
                println!("\n[cotrader] ⏹ Shutting down Kronos server...");
                // kill_on_drop will handle the subprocess when `child` drops
                drop(child);
                // Small wait for graceful shutdown
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                println!("[cotrader] Kronos server stopped.");
                return Ok(());
            }
        };

        if let Some(code) = status.code() {
            println!(
                "[kronos] ⚠ Process exited with code {}. Restarting in {:.1}s...",
                code,
                restart_delay.as_secs_f64()
            );
        } else {
            println!(
                "[kronos] ⚠ Process terminated by signal. Restarting in {:.1}s...",
                restart_delay.as_secs_f64()
            );
        }

        // Wait before restarting (exponential backoff, capped)
        tokio::time::sleep(restart_delay).await;
        restart_delay = (restart_delay * 2).min(MAX_RESTART_DELAY);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Handler: rat download-model
// ═══════════════════════════════════════════════════════════════════════════════

async fn handle_download_model() -> anyhow::Result<()> {
    let python = find_python().ok_or_else(|| {
        anyhow::anyhow!("Python not found. Install Python 3 and ensure `python3` is on your PATH.")
    })?;

    let root = find_project_root();
    let script = root.join("kronos_service").join("download.py");
    if !script.exists() {
        anyhow::bail!(
            "Download script not found at: {}\n\
             Make sure you're running from the rat project root.",
            script.display()
        );
    }

    println!("[cotrader] 🐍 Using: {} from: {}", python, script.display());

    // Optionally check/install requirements first
    let reqs = root.join("kronos_service").join("requirements.txt");
    if reqs.exists() {
        println!("[cotrader] 📦 Checking Python dependencies...");
        let check = tokio::process::Command::new(&python)
            .args(["-m", "pip", "install", "-r", &reqs.to_string_lossy()])
            .current_dir(&root)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .await
            .context("Failed to run pip install")?;

        if !check.success() {
            eprintln!("[cotrader] ⚠ pip install had issues — continuing anyway...");
        }
    }

    println!("[cotrader] ⬇ Downloading Chronos-Bolt model from HuggingFace Hub...\n");

    let status = tokio::process::Command::new(&python)
        .arg(&script)
        .current_dir(&root)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to run download.py")?;

    if status.success() {
        println!("\n[cotrader] Model downloaded successfully. Start the Kronos server with:");
        println!("   cotrader start-kronos");
        Ok(())
    } else {
        anyhow::bail!(
            "Download failed with exit code {:?}",
            status.code().unwrap_or(-1)
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Subcommand Handlers
// ═══════════════════════════════════════════════════════════════════════════════

/// Handle broker subcommands (list, configure, cache) and service commands.
async fn handle_command(cmd: &Command) -> anyhow::Result<()> {
    // Service commands exit early
    match cmd {
        Command::StartKronos { port } => return handle_start_kronos(*port).await,
        Command::Download => return handle_download_model().await,
        Command::Start { mode, symbols, tui, confirm_live } => {
            return handle_start_all(*mode, symbols.clone(), *tui, *confirm_live).await;
        }
        Command::Stop => return handle_stop().await,
        Command::Status => return handle_status().await,
        Command::Completions { shell } => {
            let shell_name = match shell {
                Shell::Bash => clap_complete::Shell::Bash,
                Shell::Zsh => clap_complete::Shell::Zsh,
                Shell::Fish => clap_complete::Shell::Fish,
                Shell::PowerShell => clap_complete::Shell::PowerShell,
                Shell::Elvish => clap_complete::Shell::Elvish,
            };
            let mut cmd = clap::Command::new("cotrader").version(env!("CARGO_PKG_VERSION"));
            clap_complete::generate(shell_name, &mut cmd, "cotrader", &mut std::io::stdout());
            return Ok(());
        }
        Command::Build { package } => {
            println!("\n  Building CoTrader...\n");
            let root = find_project_root();
            let mut cmd = tokio::process::Command::new("cargo");
            cmd.arg("build").arg("--release").current_dir(&root);
            if let Some(pkg) = package {
                cmd.args(["-p", pkg.as_str()]);
            }
            let status = cmd
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status().await
                .context("Failed to run cargo build")?;
            if status.success() {
                println!("\n  ✅ Build successful.");
                return Ok(());
            } else {
                anyhow::bail!("Build failed with exit code {:?}", status.code().unwrap_or(-1));
            }
        }
        _ => {}
    }

    let registry = BrokerPluginManager::new();
    match cmd {
        Command::List => {
            println!("\nAvailable brokers:");
            if registry.list().is_empty() {
                println!("  (No broker plugins registered)");
                println!("  Built-in: paper — virtual money broker (always available)");
            } else {
                for p in registry.list() {
                    println!("  {} — {}", p.id, p.display_name);
                    if !p.description.is_empty() {
                        println!("    {}", p.description);
                    }
                    for (key, value) in &p.config_schema {
                        println!("    {}: {}", key, value);
                    }
                    println!();
                }
            }
        }
        Command::Configure { broker_id } => {
            let plugin = registry
                .get(broker_id)
                .ok_or_else(|| anyhow::anyhow!("Unknown broker: {}", broker_id))?;

            let mut config = BrokerConfig::default();
            println!("\nConfiguring {} ({})", plugin.display_name, plugin.id);

            for (key, default_val) in &plugin.config_schema {
                print!("  {} [{}]: ", key, default_val);
                use std::io::{self, Write};
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin()
                    .read_line(&mut input)
                    .with_context(|| "Failed to read input")?;
                let value = input.trim();
                if !value.is_empty() {
                    config.set(key, value);
                } else {
                    config.set(key, default_val);
                }
            }

            // Save config to ~/.rat/{broker_id}.toml
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
            let rat_dir = home.join(".rat");
            std::fs::create_dir_all(&rat_dir).ok();
            let config_path = rat_dir.join(format!("{}.toml", broker_id));
            let toml_content = config.fields.iter()
                .map(|(k, v)| format!("{} = {:?}\n", k, v))
                .collect::<String>();
            std::fs::write(&config_path, toml_content)
                .map_err(|e| anyhow::anyhow!("Failed to save config: {}", e))?;
            println!("Configuration saved to {}", config_path.display());

            // Test connection via PluginRegistry
            println!("Testing connection...");
            let plugin_reg = cotrader_runtime::broker::plugin_registry::PluginRegistry::new();
            match plugin_reg.connect(broker_id, &config.fields).await {
                Ok(_handle) => {
                    println!("✓ {} connected successfully", plugin.display_name);
                }
                Err(e) => {
                    eprintln!("⚠ Connection failed: {}", e);
                    eprintln!("  Config was saved — fix credentials and run again.");
                }
            }
        }
        Command::Cache => {
            let paper_broker: Arc<dyn BrokerAdapter> = Arc::new(
                cotrader_runtime::broker::plugin_registry::PaperBroker::new()
            );
            let state = cotrader_autonomous::state::SharedState::new(
                cotrader_core::MemoryStore::new("rat.redb")?,
                cotrader_core::DisciplineRules::default(),
                cotrader_core::Config::default(),
                "rat_history.db",
                paper_broker,
            )?;
            let cache = cotrader_runtime::policy_cache::PolicyCache::from_disk(state);

            println!("\nPolicy Cache Health");
            println!("  Entries: {}", cache.size());
            println!("  Total samples: {}", cache.total_samples());

            let top = cache.top_performers(3, 10);
            if top.is_empty() {
                println!("  No entries with \u{2265}3 samples yet.");
                println!("  Run paper trades to populate the cache.");
            } else {
                println!("\n  Top performers (min 3 samples):");
                for e in &top {
                    println!(
                        "    {} \u{2192} {:?} | WR={:.0}% n={} conf={:.2} regret={:.3}",
                        e.features.symbol,
                        e.recommended_action,
                        e.win_rate() * 100.0,
                        e.sample_size,
                        e.confidence(),
                        e.avg_regret
                    );
                }
            }

            // Show config thresholds
            println!("\n  Thresholds:");
            println!("    min_samples: {}", cache.config().min_samples);
            println!(
                "    min_win_rate: {:.0}%",
                cache.config().min_win_rate * 100.0
            );
            println!("    min_confidence: {:.2}", cache.config().min_confidence);
        }
        // These are handled in main or above
        Command::StartKronos { .. } | Command::Download | Command::Serve { .. }
        | Command::Start { .. } | Command::Stop | Command::Status | Command::Build { .. }
        | Command::Completions { .. } => unreachable!(),
    }
    Ok(())
}

/// Try to build a live broker registry from saved config files.
/// Checks `~/.rat/{alpaca,zerodha}.toml` and registers the first found.
async fn build_live_broker_registry() -> anyhow::Result<Option<BrokerRegistry>> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let rat_dir = home.join(".rat");

    // Check for saved broker configs in priority order: alpaca, zerodha
    let broker_ids = ["alpaca", "zerodha"];
    for id in &broker_ids {
        let config_path = rat_dir.join(format!("{}.toml", id));
        if !config_path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Failed to read {}: {}", config_path.display(), e);
                continue;
            }
        };

        let values: std::collections::HashMap<String, String> = match toml::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: Failed to parse {}: {}", config_path.display(), e);
                continue;
            }
        };

        // Use PluginRegistry::connect to test the connection
        let plugin_reg = cotrader_runtime::broker::plugin_registry::PluginRegistry::new();
        match plugin_reg.connect(id, &values).await {
            Ok(handle) => {
                let paper_broker: Arc<dyn BrokerAdapter> = Arc::new(
                    cotrader_runtime::broker::plugin_registry::PaperBroker::new()
                );
                let br = BrokerRegistry::new(paper_broker);
                br.register_live_broker(Arc::from(handle.adapter))
                    .await;
                br.set_mode(cotrader_core::paper_engine::TradingMode::Live)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set live mode: {}", e))?;
                return Ok(Some(br));
            }
            Err(e) => {
                eprintln!("Warning: Failed to connect broker '{}': {}", id, e);
                continue;
            }
        }
    }

    eprintln!("No saved broker config found. Use `cotrader configure <broker_id>` first.");
    Ok(None)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    // Handle subcommands (exit early if one was provided)
    if let Some(ref cmd) = args.command {
        // For Serve, we extract the args and run the trading system
        match cmd {
            Command::Serve {
                mode,
                confirm_live,
                data,
                cycles,
                induce_regret,
                max_daily_loss,
                capital,
            } => {
                // Run the trading system with these args
                let mode_config = ModeConfig {
                    mode: *mode,
                    require_trade_confirmation: true,
                    max_daily_loss: *max_daily_loss,
                    symbol_whitelist: None,
                    backtest_start: None,
                    backtest_end: None,
                    backtest_data_path: data.clone(),
                    backtest_initial_capital: *capital,
                    validate_cycles: *cycles,
                    induce_regret: *induce_regret,
                };

                // SAFETY: live mode requires explicit confirmation
                if *mode == TradingMode::Live && !confirm_live {
                    eprintln!("\n╔══════════════════════════════════════════════════════════╗");
                    eprintln!("║  ⚠ LIVE TRADING REQUESTED BUT NOT CONFIRMED              ║");
                    eprintln!("║  You must pass --confirm-live to trade with real money.  ║");
                    eprintln!("║  Run with --mode paper for safe paper trading.            ║");
                    eprintln!("╚══════════════════════════════════════════════════════════╝\n");
                    std::process::exit(1);
                }

                if *mode == TradingMode::Backtest && data.is_none() {
                    eprintln!("Error: --data <csv_path> is required for backtest mode");
                    std::process::exit(1);
                }

                println!("{}", "╔══════════════════════════════════════════════════════════╗".cyan());
                println!("{}", "║         cotrader — Autonomous Trading System          ║".cyan().bold());
                println!("{}", "║         Trading Real-time Edge Decision Optimisation  ║".cyan());
                println!("{}", "╚══════════════════════════════════════════════════════════╝".cyan());
                println!("Mode: {}", mode.to_string().yellow().bold());

                // Initialize the system
                use std::sync::Arc;
    let paper_broker: Arc<dyn cotrader_core::paper_engine::BrokerAdapter> = {
        Arc::new(cotrader_runtime::broker::plugin_registry::PaperBroker::new())
    };
    let state = cotrader_autonomous::state::SharedState::new(
                    cotrader_core::MemoryStore::new("rat.redb")?,
                    cotrader_core::DisciplineRules::default(),
                    cotrader_core::Config::default(),
                    "rat_history.db",
                    paper_broker,
                )?;
                let mut orchestrator = AutonomousOrchestrator::new(state);
                orchestrator.init_rat();

                // Get symbols
                {
                    let mut wl = orchestrator.state.market_data.watchlist.write().await;
                    if wl.is_empty() {
                        let inferred = data
                            .as_ref()
                            .and_then(|p| std::path::Path::new(p).file_stem())
                            .and_then(|s| s.to_str())
                            .and_then(|s| s.split('_').next())
                            .unwrap_or("BTC")
                            .to_uppercase();
                        *wl = vec![inferred];                println!("[cotrader] Auto-seeded watchlist: {:?} (from backtest data)",
                            *wl
                        );
                    }
                }
                let symbols = orchestrator.state.market_data.watchlist.read().await.clone();

                // Build broker registry (for live mode)
                let broker_registry: Option<Arc<BrokerRegistry>> =
                    if *mode == TradingMode::Live {
                        match build_live_broker_registry().await {
                            Ok(Some(registry)) => {
                                println!(
                                    "✓ Live broker registered: {}",
                                    registry.current_broker_name().await
                                );
                                Some(Arc::new(registry))
                            }
                            Ok(None) => {
                                eprintln!(
                                    "⚠ No live broker configured. Use `rat configure <broker_id>` first."
                                );
                                None
                            }
                            Err(e) => {
                                eprintln!("⚠ Failed to configure live broker: {}", e);
                                eprintln!("  Falling back to paper mode for execution.");
                                None
                            }
                        }
                    } else {
                        None
                    };

                // Run
                let engine = RuntimeEngine::new(
                    mode_config,
                    orchestrator,
                    symbols,
                    broker_registry,
                )
                .await?;
                let summary = engine.run().await?;

                println!("\n=== RUN COMPLETE ===");
                println!("Mode: {}", summary.mode);
                println!("Cycles: {}", summary.cycles_completed);
                println!("Events: {}", summary.events_processed);
                println!("Trades: {}", summary.trades_executed);
                println!(
                    "Cache hits: {} (Ollama calls: {})",
                    summary.cache_hits, summary.ollama_calls
                );
                println!("P&L: ${:.2}", summary.total_pnl);
                println!("Max DD: {:.2}%", summary.max_drawdown * 100.0);
                println!("Duration: {}s", summary.duration_secs);

                return Ok(());
            }
            _ => {
                return handle_command(cmd).await;
            }
        }
    }

    // Default behavior (no subcommand) — show usage hints and exit
    // Loading the full pipeline (AutonomousOrchestrator, SharedState, ML models, etc.)
    // consumes significant memory and can trigger OOM kills. Print helpful usage
    // before any heavy allocation.
    println!("{}", "╔══════════════════════════════════════════════════════════╗".cyan());
    println!("{}", "║         cotrader — Autonomous Trading System          ║".cyan().bold());
    println!("{}", "║                                                      ║".cyan());
    println!("║  {}  Run with a subcommand to start trading.              ║", "→".dimmed());
    println!("{}", "║                                                      ║".cyan());
    println!("║  {}                                           ║", "Examples:".yellow().bold());
    println!("║    {}            Paper trading (default)  ║", "cotrader serve".green());
    println!("║    {}            Start all services       ║", "cotrader start".green());
    println!("║    {}           Check service health     ║", "cotrader status".green());
    println!("║    {}             Stop all services        ║", "cotrader stop".green());
    println!("{}", "║                                                      ║".cyan());
    println!("{}", "╚══════════════════════════════════════════════════════════╝".cyan());
    println!();
    return Ok(());

    // NOTE: The paper/default trading path was intentionally removed from
    // the bare-invocation fallback to avoid OOM-killing the process with
    // the full AutonomousOrchestrator/SharedState/RuntimeEngine pipeline.
    // Use `cotrader serve` explicitly to run paper trading.
}
