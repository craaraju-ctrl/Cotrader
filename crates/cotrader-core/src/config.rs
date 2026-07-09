use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// System operating mode — controls latency gates and telemetry verbosity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SystemMode {
    /// Normal production mode — minimal latency, standard logging.
    Production,
    /// Inspection mode — intentional latency gates, verbose CLI telemetry,
    /// long-context LLM reasoning with full chain-of-thought output.
    Inspection,
    /// Audit mode — strict sequential execution, adaptive timeout boundaries,
    /// zero-fallback drive, and comprehensive fallback causality analysis.
    Audit,
}

impl Default for SystemMode {
    fn default() -> Self {
        Self::Production
    }
}

impl SystemMode {
    /// Check if inspection mode is active (enables latency gates and verbose output).
    pub fn is_inspection(&self) -> bool {
        matches!(self, Self::Inspection)
    }

    /// Check if audit mode is active (enables sequential execution and boundary testing).
    pub fn is_audit(&self) -> bool {
        matches!(self, Self::Audit)
    }

    /// Check if any verbose mode is active (inspection or audit).
    pub fn is_verbose(&self) -> bool {
        self.is_inspection() || self.is_audit()
    }

    /// Load from environment variable `SYSTEM_MODE`.
    pub fn from_env() -> Self {
        match std::env::var("SYSTEM_MODE").unwrap_or_default().to_lowercase().as_str() {
            "inspection" | "inspect" | "debug" => Self::Inspection,
            "audit" | "boundary" | "test" => Self::Audit,
            _ => Self::Production,
        }
    }
}

/// Latency gate configuration for inspection mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyConfig {
    /// Minimum delay per validation layer (ms). Default: 1000ms (1s) in inspection mode.
    pub layer_delay_ms: u64,
    /// Maximum delay for Chronos-Bolt inference (ms). Default: 30000ms (30s).
    pub chronos_timeout_ms: u64,
    /// Maximum delay for Llama-3.2-3B reasoning (ms). Default: 60000ms (60s).
    pub llm_timeout_ms: u64,
    /// Maximum generated tokens for LLM reasoning. Default: 2048 in inspection mode.
    pub max_gen_tokens: usize,
    /// Temperature for LLM sampling. Default: 0.7 in inspection mode.
    pub temperature: f64,
    /// Top-p nucleus sampling. Default: 0.95.
    pub top_p: f64,
}

/// Audit mode configuration for sequential execution and boundary testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable strict sequential execution (Layer 1 → 2 → 3 → 4).
    pub sequential_execution: bool,
    /// Timeout per layer step (ms). Default: 5000ms (5s).
    pub layer_step_timeout_ms: u64,
    /// Timeout for Chronos-Bolt sub-steps (ms). Default: 15000ms (15s).
    pub chronos_substep_timeout_ms: u64,
    /// Timeout for LLM reasoning sub-steps (ms). Default: 30000ms (30s).
    pub llm_substep_timeout_ms: u64,
    /// Enable zero-fallback drive (attempt to avoid fallbacks).
    pub zero_fallback_drive: bool,
    /// Generate deep fallback causality analysis on timeout.
    pub fallback_causality_analysis: bool,
    /// Maximum tokens for fallback reasoning trace.
    pub fallback_reasoning_tokens: usize,
    /// Display intermediate states on timeout boundary crossing.
    pub display_boundary_states: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            sequential_execution: true,
            layer_step_timeout_ms: 5000,
            chronos_substep_timeout_ms: 15_000,
            llm_substep_timeout_ms: 30_000,
            zero_fallback_drive: true,
            fallback_causality_analysis: true,
            fallback_reasoning_tokens: 1024,
            display_boundary_states: true,
        }
    }
}

impl AuditConfig {
    /// Production mode — disabled.
    pub fn disabled() -> Self {
        Self {
            sequential_execution: false,
            layer_step_timeout_ms: 0,
            chronos_substep_timeout_ms: 0,
            llm_substep_timeout_ms: 0,
            zero_fallback_drive: false,
            fallback_causality_analysis: false,
            fallback_reasoning_tokens: 0,
            display_boundary_states: false,
        }
    }
}

/// Telemetry event for a single step within a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTelemetry {
    /// Layer name (e.g., "Rules Engine", "Chronos Forecast").
    pub layer: String,
    /// Step name within the layer (e.g., "Pivot Calculation", "VaR Gate").
    pub step: String,
    /// Start timestamp (ISO 8601).
    pub started_at: String,
    /// End timestamp (ISO 8601).
    pub completed_at: String,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
    /// Whether this step completed successfully.
    pub success: bool,
    /// Whether this step was interrupted by timeout.
    pub timeout_triggered: bool,
    /// Intermediate state at timeout boundary (if applicable).
    pub boundary_state: Option<BoundaryState>,
    /// Tool calls made during this step.
    pub tool_calls: Vec<ToolCall>,
    /// Memory/cache fetches during this step.
    pub cache_fetches: Vec<CacheFetch>,
}

/// State snapshot at the moment a timeout boundary is crossed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryState {
    /// Partial decision reached before timeout.
    pub partial_decision: Option<String>,
    /// Intermediate weights/scores at timeout.
    pub intermediate_weights: Option<Vec<(String, f64)>>,
    /// Uncompleted execution payload description.
    pub uncompleted_payload: String,
    /// Risk implications of this fallback.
    pub risk_implications: String,
}

/// A single tool invocation during step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name (e.g., "EpisodeStore::query", "CacheFrame::read").
    pub tool: String,
    /// Parameters passed to the tool.
    pub params: String,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
    /// Whether the call succeeded.
    pub success: bool,
}

/// A cache/memory fetch during step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheFetch {
    /// Cache identifier (e.g., "memory.db:decisions", "policy_cache.json").
    pub cache_id: String,
    /// Fetch type (read/write).
    pub fetch_type: String,
    /// Whether the fetch hit the cache.
    pub hit: bool,
    /// Fetch duration in milliseconds.
    pub duration_ms: u64,
}

impl Default for LatencyConfig {
    fn default() -> Self {
        Self {
            layer_delay_ms: 1000,
            chronos_timeout_ms: 30_000,
            llm_timeout_ms: 60_000,
            max_gen_tokens: 2048,
            temperature: 0.7,
            top_p: 0.95,
        }
    }
}

impl LatencyConfig {
    /// Production mode — no delays, standard LLM params.
    pub fn production() -> Self {
        Self {
            layer_delay_ms: 0,
            chronos_timeout_ms: 10_000,
            llm_timeout_ms: 30_000,
            max_gen_tokens: 128,
            temperature: 0.3,
            top_p: 0.9,
        }
    }

    /// Inspection mode — deliberate pacing, extended reasoning.
    pub fn inspection() -> Self {
        Self::default()
    }
}

/// Centralized storage configuration — single source of truth for all DB paths.
///
/// All database files are resolved relative to the project root's `storage/` directory.
/// This eliminates hardcoded relative paths scattered across the codebase.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Base directory for all persistent storage (default: "storage")
    pub base_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        // Resolve storage dir relative to the workspace root.
        // CARGO_MANIFEST_DIR points to the crate being compiled; we walk up
        // to the workspace root where `storage/` lives.
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        let base = std::path::Path::new(&manifest)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("storage");
        Self { base_dir: base }
    }
}

impl StorageConfig {
    /// Create with an explicit base directory.
    pub fn with_base(base: impl Into<PathBuf>) -> Self {
        Self { base_dir: base.into() }
    }

    /// Main trading database (episodes, orders, rules, regret, COT logs).
    pub fn main_db(&self) -> PathBuf {
        self.base_dir.join("cotrader.db")
    }

    /// Orders-only database (live order tracking).
    pub fn orders_db(&self) -> PathBuf {
        self.base_dir.join("orders.db")
    }

    /// Agentic memory server database.
    pub fn memory_db(&self) -> PathBuf {
        self.base_dir.join("memory.db")
    }

    /// Knowledge graph JSON snapshot.
    pub fn knowledge_graph(&self) -> PathBuf {
        self.base_dir.join("knowledge_graph.json")
    }

    /// Policy cache JSON snapshot.
    pub fn policy_cache(&self) -> PathBuf {
        self.base_dir.join("policy_cache.json")
    }

    /// ML model weights directory.
    pub fn model_dir(&self) -> PathBuf {
        self.base_dir.join("models")
    }

    /// Reasoning trace log.
    pub fn reasoning_log(&self) -> PathBuf {
        self.base_dir.join("reasoning.jsonl")
    }

    /// Ensure the storage directory exists.
    pub fn ensure_exists(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.base_dir)
    }
}

/// Which backend to use for LLM signal arbitration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum LlamaBackend {
    /// Use a local Ollama instance (zero RAM overhead — runs in separate process).
    Ollama {
        /// Ollama server URL, e.g. "http://localhost:11434"
        url: String,
        /// Model name, e.g. "llama3.2:3b"
        model: String,
    },
    /// Use cached GGUF via Candle (~2GB RAM, ~6s inference on CPU).
    CandleGGUF,
    /// No LLM arbitration — consensus-only fallback.
    None,
}

impl Default for LlamaBackend {
    fn default() -> Self {
        Self::None
    }
}

/// Persistable system configuration saved to `~/.rat/system.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Whether the one-time setup wizard has been completed.
    pub setup_completed: bool,
    /// Which LLM backend to use for signal arbitration.
    pub llama_backend: LlamaBackend,
    /// System operating mode (Production, Inspection, or Audit).
    #[serde(default)]
    pub system_mode: SystemMode,
    /// Latency gate configuration (used in inspection mode).
    #[serde(default)]
    pub latency_config: LatencyConfig,
    /// Audit mode configuration (used in audit mode).
    #[serde(default)]
    pub audit_config: AuditConfig,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            setup_completed: false,
            llama_backend: LlamaBackend::None,
            system_mode: SystemMode::from_env(),
            latency_config: LatencyConfig::default(),
            audit_config: AuditConfig::default(),
        }
    }
}

impl SystemConfig {
    /// Resolve the `~/.rat/` directory, creating it if needed.
    pub fn rat_dir() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let dir = home.join(".rat");
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    /// Path to the system config file.
    pub fn path() -> PathBuf {
        Self::rat_dir().join("system.toml")
    }

    /// Load system config from disk, returning default if not found.
    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                toml::from_str(&content).unwrap_or_else(|e| {
                    eprintln!("[SystemConfig] ⚠ Failed to parse {}: {}. Using defaults.", path.display(), e);
                    Self::default()
                })
            }
            Err(e) => {
                eprintln!("[SystemConfig] ⚠ Failed to read {}: {}. Using defaults.", path.display(), e);
                Self::default()
            }
        }
    }

    /// Save system config to disk.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = Self::path();
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub initial_balance: f64,
    pub max_position_size: f64,
    pub api_key: String,
    pub api_secret: String,

    // === Notifications (WhatsApp / Telegram) ===
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
    pub whatsapp_sid: String,
    pub whatsapp_token: String,
    pub whatsapp_from: String,

    // === Real-time Tools ===
    pub ws_enabled: bool,
    pub web_api_addr: String,
    pub ws_port: u16,

    // === News ===
    pub newsapi_key: String,
    pub alpha_vantage_key: String,
    pub finnhub_key: String,
    pub marketaux_key: String,

    // === More free/fremium APIs (research 2026: Polygon for aggs+indicators, FRED for macro metrics, CoinGecko keyless/public for crypto) ===
    pub polygon_api_key: String,
    pub fred_api_key: String,

    // Paper enforcement (set by launcher/setup)
    pub paper_mode: bool,

    // === LLM/Model Backend (from system config) ===
    pub llama_backend: LlamaBackend,
    pub setup_completed: bool,

    // === System Mode & Latency Gates ===
    pub system_mode: SystemMode,
    pub latency_config: LatencyConfig,
    pub audit_config: AuditConfig,
}

impl Default for Config {
    fn default() -> Self {
        let paper_mode = std::env::var("PAPER_MODE")
            .map(|v| v != "false")
            .unwrap_or(true);

        // Merge system config from disk
        let sys = SystemConfig::load();

        Self {
            initial_balance: 100_000.0,
            max_position_size: if paper_mode { 0.95 } else { 0.04 },
            api_key: "DUMMY_API_KEY".to_string(),
            api_secret: "DUMMY_API_SECRET".to_string(),

            telegram_bot_token: std::env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default(),
            telegram_chat_id: std::env::var("TELEGRAM_CHAT_ID").unwrap_or_default(),
            whatsapp_sid: std::env::var("WHATSAPP_SID").unwrap_or_default(),
            whatsapp_token: std::env::var("WHATSAPP_TOKEN").unwrap_or_default(),
            whatsapp_from: std::env::var("WHATSAPP_FROM").unwrap_or_default(),

            ws_enabled: std::env::var("WS_ENABLED")
                .map(|v| v == "true" || v == "Y" || v == "y")
                .unwrap_or(true),
            web_api_addr: std::env::var("WEB_API_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8082".to_string()),
            ws_port: std::env::var("WS_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8082),

            newsapi_key: std::env::var("NEWSAPI_KEY").unwrap_or_default(),
            alpha_vantage_key: std::env::var("ALPHA_VANTAGE_KEY").unwrap_or_default(),
            finnhub_key: std::env::var("FINNHUB_KEY").unwrap_or_default(),
            marketaux_key: std::env::var("MARKETAUX_KEY").unwrap_or_default(),

            polygon_api_key: std::env::var("POLYGON_API_KEY").unwrap_or_default(),
            fred_api_key: std::env::var("FRED_API_KEY").unwrap_or_default(),

            paper_mode,
            llama_backend: sys.llama_backend.clone(),
            setup_completed: sys.setup_completed,
            system_mode: sys.system_mode.clone(),
            latency_config: if sys.system_mode.is_verbose() {
                sys.latency_config.clone()
            } else {
                LatencyConfig::production()
            },
            audit_config: if sys.system_mode.is_audit() {
                sys.audit_config.clone()
            } else {
                AuditConfig::disabled()
            },
        }
    }
}

impl Config {
    /// Load from env (populated by `source config/rat.env` after `./rat setup`).
    pub fn load() -> Self {
        Self::default()
    }
}
