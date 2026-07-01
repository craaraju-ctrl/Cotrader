//! rat — Autonomous Trading System
//!
//! Usage:
//!     rat                                   # Run paper trading (default)
//!     rat serve                             # Run paper trading (alias)
//!     rat serve --mode live --confirm-live  # Live trading (requires confirmation)
//!     rat serve --mode backtest --data ./data.csv --capital 100000
//!     rat serve --mode validate --cycles 100
//!     rat serve --mode research             # Observe market, no trading
//!     rat start                             # Start Kronos forecast server
//!     rat start --port 8000                 # Start Kronos on custom port
//!     rat download                          # Download Chronos model
//!     rat list                              # List available brokers
//!     rat configure <broker_id>             # Configure a broker
//!     rat cache                             # Show policy cache stats
//!
//! All modes share the same agent core, so backtested strategies = live strategies.

use anyhow::Context;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use rat_autonomous::AutonomousOrchestrator;
use rat_core::paper_engine::{BrokerRegistry, PaperEngineConfig};
use rat_runtime::broker::{BrokerConfig, BrokerPluginManager};
use rat_runtime::engine::RuntimeEngine;
use rat_runtime::mode::{ModeConfig, TradingMode};

#[derive(Parser, Debug)]
#[command(
    name = "rat",
    version,
    about = "rat — Autonomous Trading System",
    long_about = "rat: Trading Real-time Edge Decision Optimisation\n\nA production-grade, Rust-first autonomous agentic trading co-pilot."
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Trading mode
    #[arg(long, default_value_t = TradingMode::Paper)]
    mode: TradingMode,

    /// REQUIRED for live mode: explicit confirmation
    #[arg(long, default_value_t = false)]
    confirm_live: bool,

    /// Required for backtest mode: path to CSV (timestamp,open,high,low,close,volume)
    #[arg(long)]
    data: Option<String>,

    /// Required for validate mode: number of cycles
    #[arg(long, default_value_t = 50)]
    cycles: usize,

    /// For validate mode: induce regret to force rule adaptation
    #[arg(long, default_value_t = false)]
    induce_regret: bool,

    /// Max daily loss in currency (default 1000)
    #[arg(long, default_value_t = 1000.0)]
    max_daily_loss: f64,

    /// Starting capital for backtest (default 100000)
    #[arg(long, default_value_t = 100_000.0)]
    capital: f64,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start all services: memory, pipeline, orchestrator, optional TUI
    Start {
        /// Trading mode: paper, live
        #[arg(long, default_value_t = TradingMode::Paper)]
        mode: TradingMode,

        /// Symbols to trade (comma-separated)
        #[arg(long, default_value_t = String::from("BTC,ETH"))]
        symbols: String,

        /// Launch the terminal UI after services start
        #[arg(long, default_value_t = false)]
        tui: bool,

        /// REQUIRED for live mode: explicit confirmation
        #[arg(long, default_value_t = false)]
        confirm_live: bool,

        /// Skip memory server (if already running externally)
        #[arg(long, default_value_t = false)]
        no_memory: bool,

        /// Skip orchestrator (run pipeline only)
        #[arg(long, default_value_t = false)]
        no_orchestrator: bool,
    },

    /// Stop all running services
    Stop,

    /// Show status of all services
    Status,

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
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helper: find the project root directory containing kronos_service/
// ═══════════════════════════════════════════════════════════════════════════════

fn find_project_root() -> std::path::PathBuf {
    // First, try CARGO_MANIFEST_DIR (set by cargo build/run)
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = std::path::PathBuf::from(&manifest);
        // CARGO_MANIFEST_DIR is .../crates/rat-runtime; project root is ../..
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
    eprintln!("[rat] Could not locate kronos_service/ directory. Using current directory.");
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
            println!("  [OK] {} listening on port {}", name, port);
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if i % 5 == 4 {
            println!("  [..] Waiting for {} on port {} ({}s)...", name, port, i + 1);
        }
    }
    println!("  [WARN] {} did not respond on port {} after {}s", name, port, max_secs);
    false
}

async fn handle_start_all(
    mode: TradingMode,
    symbols: String,
    launch_tui: bool,
    confirm_live: bool,
    no_memory: bool,
    no_orchestrator: bool,
) -> anyhow::Result<()> {
    let root = find_project_root();
    let logs = pid_dir();
    let memory_pid = logs.join("memory.pid");
    let orchestrator_pid = logs.join("orchestrator.pid");
    let pipeline_pid = logs.join("pipeline.pid");

    println!();
    println!("  RAT Agent — Starting All Services");
    println!("  Mode: {}  |  Symbols: {}", mode, symbols);
    println!("  Services: Memory, LLM (Ollama), Kronos, Orchestrator, Pipeline");
    println!();

    // Track child processes for graceful shutdown
    let mut children: Vec<(String, tokio::process::Child)> = Vec::new();

    // ── 0. Ollama LLM Server ────────────────────────────────────────────
    {
        // Check if Ollama is already running
        let ollama_running = reqwest::Client::new()
            .get("http://localhost:11434/api/tags")
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);

        if ollama_running {
            println!("  [OK] Ollama LLM already running (port 11434)");
        } else {
            println!("  [..] Starting Ollama LLM server...");
            let child = tokio::process::Command::new("ollama")
                .args(["serve"])
                .stdout(std::fs::File::create(logs.join("ollama.log")).unwrap())
                .stderr(std::fs::File::create(logs.join("ollama.err")).unwrap())
                .kill_on_drop(true)
                .spawn();
            match child {
                Ok(c) => {
                    children.push(("ollama".into(), c));
                    wait_for_port(11434, "Ollama LLM", 30).await;
                }
                Err(_) => println!("  [WARN] Ollama not found — install from ollama.com"),
            }
        }
    }

    // ── 1. Memory Server (port 3111) ──────────────────────────────────────
    if !no_memory {
        if is_running(&memory_pid) {
            println!("  [SKIP] Agentic Memory already running (PID {})",
                std::fs::read_to_string(&memory_pid).unwrap_or_default().trim());
        } else {
            println!("  [..] Starting Agentic Memory on port 3111...");
            let log_file = logs.join("memory.log");
            let child = tokio::process::Command::new("cargo")
                .args(["run", "--release", "-p", "agentic-memory"])
                .current_dir(&root)
                .stdout(std::fs::File::create(&log_file).unwrap())
                .stderr(std::fs::File::create(logs.join("memory.err")).unwrap())
                .kill_on_drop(true)
                .spawn()
                .context("Failed to spawn agentic-memory")?;
            write_pid(&memory_pid, child.id().unwrap_or(0));
            children.push(("memory".into(), child));
            wait_for_port(3111, "Agentic Memory", 45).await;
        }
    }

    // ── 2. Orchestrator (port 8082) ────────────────────────────────────────
    if !no_orchestrator {
        if is_running(&orchestrator_pid) {
            println!("  [SKIP] Orchestrator already running (PID {})",
                std::fs::read_to_string(&orchestrator_pid).unwrap_or_default().trim());
        } else {
            println!("  [..] Starting Orchestrator...");
            let log_file = logs.join("orchestrator.log");
            let child = tokio::process::Command::new("cargo")
                .args(["run", "--release", "-p", "rat-orchestrator"])
                .current_dir(&root)
                .env("MEMORY_API_URL", "http://localhost:3111")
                .env("WEB_API_ADDR", "0.0.0.0:8082")
                .env("RUST_LOG", "info")
                .stdout(std::fs::File::create(&log_file).unwrap())
                .stderr(std::fs::File::create(logs.join("orchestrator.err")).unwrap())
                .kill_on_drop(true)
                .spawn()
                .context("Failed to spawn rat-orchestrator")?;
            write_pid(&orchestrator_pid, child.id().unwrap_or(0));
            children.push(("orchestrator".into(), child));
            wait_for_port(8081, "Orchestrator", 120).await;
        }
    }

    // ── 3. Pipeline (paper/live trading) ───────────────────────────────────
    if is_running(&pipeline_pid) {
        println!("  [SKIP] Pipeline already running (PID {})",
            std::fs::read_to_string(&pipeline_pid).unwrap_or_default().trim());
    } else {
        println!("  [..] Starting Pipeline ({}, symbols: {})...", mode, symbols);
        let log_file = logs.join("pipeline.log");
        let mode_str = mode.to_string().to_lowercase();
        let mut args = vec![
            "--symbols", &symbols,
            "--mode", &mode_str,
        ];
        if mode == TradingMode::Live && confirm_live {
            args.push("--confirm-live");
        }
        let child = tokio::process::Command::new("cargo")
            .args(["run", "--release", "-p", "rat-pipeline", "--"])
            .args(&args)
            .current_dir(&root)
            .env("MEMORY_API_URL", "http://localhost:3111")
            .stdout(std::fs::File::create(&log_file).unwrap())
            .stderr(std::fs::File::create(logs.join("pipeline.err")).unwrap())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn rat-pipeline")?;
        write_pid(&pipeline_pid, child.id().unwrap_or(0));
        children.push(("pipeline".into(), child));
        // Pipeline connects to Binance — give it a few seconds
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        println!("  [OK] Pipeline started");
    }

    println!();
    println!("  All services running. Press Ctrl+C to stop.");
    println!("  Logs: {}/", logs.display());
    println!("  Status: rat status  |  Stop: rat stop");
    println!();

    // Optionally launch TUI
    if launch_tui {
        println!("  Launching TUI...");
        let tui_child = tokio::process::Command::new("cargo")
            .args(["run", "--release", "-p", "rat-tui"])
            .current_dir(&root)
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn rat-tui")?;
        children.push(("tui".into(), tui_child));
    }

    // ── Wait for Ctrl+C ────────────────────────────────────────────────────
    println!("  Press Ctrl+C to stop all services.");
    let _ = tokio::signal::ctrl_c().await;
    println!();
    println!("  Shutting down all services...");

    // Send SIGTERM to all children
    for (name, child) in &mut children {
        println!("  Stopping {}...", name);
        child.kill().await.ok();
    }

    // Clean up PID files
    remove_pid(&memory_pid);
    remove_pid(&orchestrator_pid);
    remove_pid(&pipeline_pid);

    println!("  All services stopped.");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Handler: rat stop — stop all running services
// ═══════════════════════════════════════════════════════════════════════════════

async fn handle_stop() -> anyhow::Result<()> {
    let logs = pid_dir();
    let services = [
        ("memory", logs.join("memory.pid")),
        ("orchestrator", logs.join("orchestrator.pid")),
        ("pipeline", logs.join("pipeline.pid")),
    ];

    println!();
    for (name, pid_file) in &services {
        if is_running(pid_file) {
            let pid_str = std::fs::read_to_string(pid_file).unwrap_or_default();
            let pid: i32 = pid_str.trim().parse().unwrap_or(0);
            unsafe { libc::kill(pid, libc::SIGTERM) };
            println!("  Stopped {} (PID {})", name, pid);
            remove_pid(pid_file);
        } else {
            println!("  {} not running", name);
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
    let services = [
        ("Agentic Memory  ", logs.join("memory.pid"), 3111),
        ("Orchestrator    ", logs.join("orchestrator.pid"), 8082),
        ("Pipeline        ", logs.join("pipeline.pid"), 0),
    ];

    println!();
    println!("  RAT Agent — Service Status");
    println!("  ─────────────────────────────────");
    for (name, pid_file, port) in &services {
        let (status, pid_display) = if is_running(pid_file) {
            let pid = std::fs::read_to_string(pid_file).unwrap_or_default().trim().to_string();
            let port_info = if *port > 0 {
                format!(" (port {})", port)
            } else {
                String::new()
            };
            (format!("RUNNING{}", port_info), format!("PID {}", pid))
        } else {
            ("STOPPED".into(), String::new())
        };
        println!("  {} : {} {}", name, status, pid_display);
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
    println!("[rat] 🐍 Using: {} from: {}", python, script.display());
    println!(
        "[rat] 🌐 Starting Kronos forecast server on port {}...",
        port
    );
    println!("[rat]    To stop: Ctrl+C\n");

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
        println!("[kronos] PID {} started", pid);

        // Spawn stdout reader
        let stdout = child.stdout.take();
        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    println!("[kronos] {}", line);
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
                    eprintln!("[kronos:err] {}", line);
                }
            });
        }

        // Wait for the child to exit OR for Ctrl+C
        let status = tokio::select! {
            status = child.wait() => status?,
            _ = tokio::signal::ctrl_c() => {
                println!("\n[rat] ⏹ Shutting down Kronos server...");
                // kill_on_drop will handle the subprocess when `child` drops
                drop(child);
                // Small wait for graceful shutdown
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                println!("[rat] ✅ Kronos server stopped.");
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

    println!("[rat] 🐍 Using: {} from: {}", python, script.display());

    // Optionally check/install requirements first
    let reqs = root.join("kronos_service").join("requirements.txt");
    if reqs.exists() {
        println!("[rat] 📦 Checking Python dependencies...");
        let check = tokio::process::Command::new(&python)
            .args(["-m", "pip", "install", "-r", &reqs.to_string_lossy()])
            .current_dir(&root)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .await
            .context("Failed to run pip install")?;

        if !check.success() {
            eprintln!("[rat] ⚠ pip install had issues — continuing anyway...");
        }
    }

    println!("[rat] ⬇ Downloading Chronos-Bolt model from HuggingFace Hub...\n");

    let status = tokio::process::Command::new(&python)
        .arg(&script)
        .current_dir(&root)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to run download.py")?;

    if status.success() {
        println!("\n[rat] ✅ Model downloaded successfully. Start the Kronos server with:");
        println!("   rat start-kronos");
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
        Command::Start { mode, symbols, tui, confirm_live, no_memory, no_orchestrator } => {
            return handle_start_all(*mode, symbols.clone(), *tui, *confirm_live, *no_memory, *no_orchestrator).await;
        }
        Command::Stop => return handle_stop().await,
        Command::Status => return handle_status().await,
        _ => {}
    }

    let registry = BrokerPluginManager::new();
    match cmd {
        Command::List => {
            println!("\nAvailable brokers:");
            for p in registry.list() {
                println!("  {} — {}", p.id, p.display_name);
                if !p.description.is_empty() {
                    println!("    {}", p.description);
                }
                for field in &p.config_schema {
                    let sensitive = if field.sensitive { " (sensitive)" } else { "" };
                    let default = field.default.as_deref().unwrap_or("(required)");
                    println!(
                        "    {} [{}]: {}{}",
                        field.key, default, field.label, sensitive
                    );
                }
                println!();
            }
        }
        Command::Configure { broker_id } => {
            let plugin = registry
                .get(broker_id)
                .ok_or_else(|| anyhow::anyhow!("Unknown broker: {}", broker_id))?;

            let mut config = BrokerConfig::default();
            println!("\nConfiguring {} ({})", plugin.display_name, plugin.id);

            for field in &plugin.config_schema {
                let prompt = if field.sensitive {
                    format!("  {} (hidden, or set via env var): ", field.label)
                } else {
                    format!(
                        "  {} [{}]: ",
                        field.label,
                        field.default.as_deref().unwrap_or("")
                    )
                };
                print!("{}", prompt);
                use std::io::{self, Write};
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin()
                    .read_line(&mut input)
                    .with_context(|| "Failed to read input")?;
                let value = input.trim();
                if !value.is_empty() {
                    config.set(&field.key, value);
                } else if let Some(default) = &field.default {
                    config.set(&field.key, default);
                }
            }

            // Save config
            registry
                .save_config(broker_id, &config)
                .map_err(|e| anyhow::anyhow!("Failed to save config for {}: {}", broker_id, e))?;
            println!("Configuration saved to ~/.rat/{}.toml", broker_id);

            // Test connection
            println!("Testing connection...");
            match registry.instantiate(broker_id, &config).await {
                Ok(handle) => {
                    println!("✓ {} connected successfully", handle.plugin.display_name);
                }
                Err(e) => {
                    eprintln!("⚠ Connection failed: {}", e);
                    eprintln!("  Config was saved — fix credentials and run again.");
                }
            }
        }
        Command::Cache => {
            let state = rat_autonomous::state::SharedState::new(
                rat_core::MemoryStore::new("rat.redb")?,
                rat_core::DisciplineRules::default(),
                rat_core::Config::default(),
                "rat_history.db",
            )?;
            let cache = rat_runtime::policy_cache::PolicyCache::from_disk(state);

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
        | Command::Start { .. } | Command::Stop | Command::Status => unreachable!(),
    }
    Ok(())
}

/// Try to build a live broker registry from saved config files.
/// Checks `~/.rat/{alpaca,zerodha}.toml` and registers the first found.
async fn build_live_broker_registry() -> anyhow::Result<Option<BrokerRegistry>> {
    let registry = BrokerPluginManager::new();
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

        let mut config = BrokerConfig::default();
        for (k, v) in &values {
            config.set(k, v);
        }

        match registry.instantiate(id, &config).await {
            Ok(handle) => {
                let br = BrokerRegistry::new(PaperEngineConfig::default());
                br.register_live_broker(std::sync::Arc::from(handle.adapter))
                    .await;
                br.set_mode(rat_core::paper_engine::TradingMode::Live)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set live mode: {}", e))?;
                return Ok(Some(br));
            }
            Err(e) => {
                eprintln!("Warning: Failed to instantiate broker '{}': {}", id, e);
                continue;
            }
        }
    }

    eprintln!("No saved broker config found. Use `rat configure <broker_id>` first.");
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

                println!("╔══════════════════════════════════════════════════════════╗");
                println!("║            rat — Autonomous Trading System              ║");
                println!("║         Trading Real-time Edge Decision Optimisation    ║");
                println!("╚══════════════════════════════════════════════════════════╝");
                println!("Mode: {}", mode);

                // Initialize the system
                let state = rat_autonomous::state::SharedState::new(
                    rat_core::MemoryStore::new("rat.redb")?,
                    rat_core::DisciplineRules::default(),
                    rat_core::Config::default(),
                    "rat_history.db",
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
                        *wl = vec![inferred];
                        println!(
                            "[rat] Auto-seeded watchlist: {:?} (from backtest data)",
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

    // Default behavior (no subcommand) — run paper trading
    let mode = args.mode;
    let confirm_live = args.confirm_live;

    // SAFETY: live mode requires explicit confirmation
    if mode == TradingMode::Live && !confirm_live {
        eprintln!("\n╔══════════════════════════════════════════════════════════╗");
        eprintln!("║  ⚠ LIVE TRADING REQUESTED BUT NOT CONFIRMED              ║");
        eprintln!("║  You must pass --confirm-live to trade with real money.  ║");
        eprintln!("║  Run with --mode paper for safe paper trading.            ║");
        eprintln!("╚══════════════════════════════════════════════════════════╝\n");
        std::process::exit(1);
    }

    if mode == TradingMode::Backtest && args.data.is_none() {
        eprintln!("Error: --data <csv_path> is required for backtest mode");
        std::process::exit(1);
    }

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║            rat — Autonomous Trading System              ║");
    println!("║         Trading Real-time Edge Decision Optimisation    ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!("Mode: {}", mode);

    // === Initialize the system ===
    let state = rat_autonomous::state::SharedState::new(
        rat_core::MemoryStore::new("rat.redb")?,
        rat_core::DisciplineRules::default(),
        rat_core::Config::default(),
        "rat_history.db",
    )?;
    let mut orchestrator = AutonomousOrchestrator::new(state);
    orchestrator.init_rat();

    // Get symbols — auto-seed if empty (critical for backtest mode)
    {
        let mut wl = orchestrator.state.market_data.watchlist.write().await;
        if wl.is_empty() {
            // Infer symbol from data filename or default to BTC
            let inferred = args
                .data
                .as_ref()
                .and_then(|p| std::path::Path::new(p).file_stem())
                .and_then(|s| s.to_str())
                .and_then(|s| s.split('_').next())
                .unwrap_or("BTC")
                .to_uppercase();
            *wl = vec![inferred];
            println!(
                "[rat] Auto-seeded watchlist: {:?} (from backtest data)",
                *wl
            );
        }
    }
    let symbols = orchestrator.state.market_data.watchlist.read().await.clone();

    // === Build mode config ===
    let mode_config = ModeConfig {
        mode: args.mode,
        require_trade_confirmation: true,
        max_daily_loss: args.max_daily_loss,
        symbol_whitelist: None,
        backtest_start: None,
        backtest_end: None,
        backtest_data_path: args.data,
        backtest_initial_capital: args.capital,
        validate_cycles: args.cycles,
        induce_regret: args.induce_regret,
    };

    // === Build broker registry (for live mode, loads saved config) ===
    let broker_registry: Option<Arc<BrokerRegistry>> = if args.mode == TradingMode::Live {
        match build_live_broker_registry().await {
            Ok(Some(registry)) => {
                println!(
                    "✓ Live broker registered: {}",
                    registry.current_broker_name().await
                );
                Some(Arc::new(registry))
            }
            Ok(None) => {
                eprintln!("⚠ No live broker configured. Use `rat configure <broker_id>` first.");
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

    // === Run ===
    let engine = RuntimeEngine::new(mode_config, orchestrator, symbols, broker_registry).await?;
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

    Ok(())
}
