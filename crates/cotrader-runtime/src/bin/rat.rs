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
//!     cotrader download                      # Download Chronos-Bolt model
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
use cotrader_ml::models::chronos_bolt;
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

    /// Download the Chronos-Bolt model from HuggingFace Hub
    Download,

    /// Download the Llama-3.2-3B reasoning engine (GGUF, ~2GB)
    DownloadLlm,

    /// Run the bootstrap setup wizard (model backend selection)
    Setup {
        /// Force re-run even if setup_completed
        #[arg(long, default_value_t = false)]
        force: bool,
    },

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
// Helper: find the project root directory
// ═══════════════════════════════════════════════════════════════════════════════

fn find_project_root() -> std::path::PathBuf {
    // First, try CARGO_MANIFEST_DIR (set by cargo build/run)
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = std::path::PathBuf::from(&manifest);
        // CARGO_MANIFEST_DIR is .../crates/cotrader-runtime; project root is ../..
        if let Some(root) = p.parent().and_then(|p| p.parent()) {
            return root.to_path_buf();
        }
    }

    // Try current working directory
    std::env::current_dir().unwrap_or_default()
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



// ═══════════════════════════════════════════════════════════════════════════════
// Subcommand Handlers
// ═══════════════════════════════════════════════════════════════════════════════

/// Handle broker subcommands (list, configure, cache) and service commands.
async fn handle_command(cmd: &Command) -> anyhow::Result<()> {
    // Service commands exit early
    match cmd {
        Command::Start { mode, symbols, tui, confirm_live } => {
            return handle_start_all(*mode, symbols.clone(), *tui, *confirm_live).await;
        }
        Command::Stop => return handle_stop().await,
        Command::Status => return handle_status().await,
        Command::Download => {
            println!("\n  Downloading Chronos-Bolt model from HuggingFace Hub...\n");
            match chronos_bolt::download_model() {
                Ok(path) => {
                    println!("  ✅ Model downloaded and cached at: {}", path);
                    // Load into global CHRONOS_MODEL for immediate use by the trend layer
                    let _ = cotrader_autonomous::tri_level_validator::load_chronos_global();
                    println!("  ✅ Chronos-Bolt model loaded and ready for inference.\n");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("  ❌ Download failed: {}", e);
                    anyhow::bail!("Model download failed");
                }
            }
        }
        Command::Setup { force } => {
            let sys = cotrader_core::config::SystemConfig::load();
            if sys.setup_completed && !force {
                println!("\n  Setup already completed. Use --force to re-run: `cotrader setup --force`\n");
                println!("  Current backend: {:?}", sys.llama_backend);
                return Ok(());
            }
            println!("\n  Running bootstrap setup wizard...\n");
            match cotrader_autonomous::setup::run_setup_wizard() {
                Ok(_) => {
                    println!("  ✅ Setup completed. Run `cotrader serve` to start trading.\n");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("  ❌ Setup failed: {}", e);
                    anyhow::bail!("Setup failed");
                }
            }
        }
        Command::DownloadLlm => {
            // Check if Ollama is available first — suggest using it
            let ollama_models = cotrader_autonomous::setup::discover_ollama_models("http://localhost:11434");
            match ollama_models {
                Ok(models) if !models.is_empty() => {
                    let suitable = cotrader_autonomous::setup::filter_suitable_models(&models);
                    if let Some(best) = suitable.first() {
                        println!("\n  ✅ Ollama detected with compatible model: {}", best.name);
                        println!("     (zero additional download needed, zero RAM overhead)");
                        println!("     Run `cotrader setup` to switch to Ollama backend.\n");
                    }
                }
                _ => {
                    println!("  ℹ No Ollama instance detected.");
                }
            }

            println!("  Downloading Llama-3.2-3B reasoning engine (GGUF, ~2GB)...");
            println!("  This will take a few minutes depending on your connection.\n");
            match cotrader_ml::models::reasoning_engine::download_model() {
                Ok(path) => {
                    println!("  ✅ LLM model downloaded and cached at: {}", path);
                    // Load into global LLM backend for immediate use
                    let _ = cotrader_autonomous::tri_level_validator::load_llm_global();
                    println!("  ✅ LLM reasoning engine loaded and ready for arbitration.\n");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("  ❌ Download failed: {}", e);
                    anyhow::bail!("LLM model download failed");
                }
            }
        }
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
            let storage = cotrader_core::config::StorageConfig::default();
            let state = cotrader_autonomous::state::SharedState::new(
                cotrader_core::MemoryStore::new(&*storage.memory_db().to_string_lossy())?,
                cotrader_core::DisciplineRules::default(),
                cotrader_core::Config::default(),
                &storage.main_db().to_string_lossy(),
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
        Command::Download | Command::DownloadLlm | Command::Serve { .. } | Command::Start { .. } | Command::Stop
        | Command::Status | Command::Build { .. } | Command::Completions { .. } | Command::Setup { .. } => unreachable!(),
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

    // Check for Tredo Exchange via env var (COTRADER_BASE_URL)
    if let Ok(tredo_url) = std::env::var("COTRADER_BASE_URL") {
        if !tredo_url.is_empty() {
            eprintln!("Found Tredo Exchange at {}", tredo_url);
            let tredo_broker: Arc<dyn BrokerAdapter> = Arc::new(
                cotrader_runtime::broker::plugin_registry::TredoBroker::from_env()
            );
            match tredo_broker.connect().await {
                Ok(()) => {
                    let paper_broker: Arc<dyn BrokerAdapter> = Arc::new(
                        cotrader_runtime::broker::plugin_registry::PaperBroker::new()
                    );
                    let br = BrokerRegistry::new(paper_broker);
                    br.register_live_broker(tredo_broker).await;
                    br.set_mode(cotrader_core::paper_engine::TradingMode::Live)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to set live mode: {}", e))?;
                    eprintln!("Tredo Exchange connected — live trading enabled");
                    return Ok(Some(br));
                }
                Err(e) => {
                    eprintln!("Warning: Failed to connect Tredo Exchange: {}", e);
                }
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
                let mut mode_config = ModeConfig {
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
    let storage = cotrader_core::config::StorageConfig::default();
    let state = cotrader_autonomous::state::SharedState::new(
                    cotrader_core::MemoryStore::new(&*storage.memory_db().to_string_lossy())?,
                    cotrader_core::DisciplineRules::default(),
                    cotrader_core::Config::default(),
                    &storage.main_db().to_string_lossy(),
                    paper_broker,
                )?;
                // ── Bootstrap & Setup Mode ────────────────────────────
                let system_config = cotrader_core::config::SystemConfig::load();
                if !system_config.setup_completed {
                    eprintln!("  First boot detected — launching setup wizard...");
                    match cotrader_autonomous::setup::run_setup_wizard() {
                        Ok(_) => {
                            // Wizard saved config to disk; fresh Config picks it up
                            let fresh_config = cotrader_core::Config::load();
                            if fresh_config.llama_backend == cotrader_core::config::LlamaBackend::CandleGGUF {
                                let _ = cotrader_autonomous::tri_level_validator::load_chronos_global();
                            }
                            let _ = cotrader_autonomous::tri_level_validator::load_llm_from_config(&fresh_config);
                        }
                        Err(e) => {
                            eprintln!("  ⚠ Setup wizard failed: {} (run `cotrader setup --force` to retry)", e);
                        }
                    }
                } else {
                    // Eager-load AI models according to saved config
                    if let Err(e) = cotrader_autonomous::tri_level_validator::load_chronos_global() {
                        eprintln!("  ⚠ Chronos-Bolt: {} (trend layer uses fallback)", e);
                    }
                    if let Err(e) = cotrader_autonomous::tri_level_validator::load_llm_from_config(&cotrader_core::Config::load()) {
                        eprintln!("  ⚠ LLM backend: {} (run `cotrader setup --force` to reconfigure)", e);
                    }
                }

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

                // Build broker registry — auto-detect TredoExchange when COTRADER_BASE_URL is set
                let broker_registry: Option<Arc<BrokerRegistry>> = {
                    let should_try_live = *mode == TradingMode::Live
                        || std::env::var("COTRADER_BASE_URL").is_ok();
                    if should_try_live {
                        match build_live_broker_registry().await {
                            Ok(Some(registry)) => {
                                let name = registry.current_broker_name().await;
                                println!("✓ Live broker registered: {}", name);
                                // Auto-upgrade mode to Live when broker connects
                                mode_config.mode = TradingMode::Live;
                                let arc_reg = Arc::new(registry);
                                // Sync the live registry into the orchestrator's state
                                // so ExecutionCoordinator reads the correct mode
                                {
                                    let state_reg = &orchestrator.state.portfolio_store.broker_registry;
                                    // Replace the PaperBroker inside the state's registry
                                    // with the live TredoBroker
                                    let live = arc_reg.live_broker().await.expect("live broker must exist");
                                    state_reg.register_live_broker(live).await;
                                    state_reg.set_mode(cotrader_core::paper_engine::TradingMode::Live).await.ok();
                                }
                                Some(arc_reg)
                            }
                            Ok(None) => {
                                if *mode == TradingMode::Live {
                                    eprintln!(
                                        "⚠ No live broker configured. Use `cotrader configure <broker_id>` first."
                                    );
                                }
                                None
                            }
                            Err(e) => {
                                eprintln!("⚠ Failed to configure live broker: {}", e);
                                if *mode == TradingMode::Live {
                                    eprintln!("  Falling back to paper mode for execution.");
                                }
                                None
                            }
                        }
                    } else {
                        None
                    }
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
