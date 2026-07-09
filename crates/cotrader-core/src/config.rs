use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            setup_completed: false,
            llama_backend: LlamaBackend::None,

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
            llama_backend: sys.llama_backend,
            setup_completed: sys.setup_completed,
        }
    }
}

impl Config {
    /// Load from env (populated by `source config/rat.env` after `./rat setup`).
    pub fn load() -> Self {
        Self::default()
    }
}
