//! Bootstrap & Setup Mode — one-time interactive wizard for model backend selection.
//!
//! On first boot (or `cotrader setup --force`), this module:
//!   1. Scans for a local Ollama instance (`GET /api/tags`)
//!   2. Checks if GGUF models are cached in HuggingFace Hub
//!   3. Presents the user with an interactive selection menu
//!   4. Persists the choice to `~/.rat/system.toml`
//!
//! On subsequent boots, the system skips the wizard and loads the saved backend.

use std::io::{self, BufRead, Write};
use serde::Deserialize;

// ── Ollama Discovery ─────────────────────────────────────────────────────────

/// A model reported by Ollama's `/api/tags` endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    #[allow(dead_code)]
    pub modified_at: Option<String>,
    #[allow(dead_code)]
    pub size: Option<u64>,
}

/// Response from `GET /api/tags`.
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

/// Models we consider suitable for signal arbitration.
/// Prioritizes instruction-tuned models that can handle structured output.
const PREFERRED_MODELS: &[&str] = &[
    "llama3.2", "llama3.1", "llama3",
    "qwen3.5", "qwen3", "qwen2.5",
    "nemotron", "nemotron-mini",
    "mistral", "mixtral",
    "phi3", "phi-3",
    "gemma2", "gemma",
];

/// Try to discover Ollama models on the local machine.
///
/// Returns `Ok(Some(models))` if Ollama is running and responds,
/// `Ok(None)` if Ollama is not reachable, or `Err` on failure.
pub fn discover_ollama_models(url: &str) -> Result<Vec<OllamaModelInfo>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let tags_url = format!("{}/api/tags", url.trim_end_matches('/'));

    let resp = client
        .get(&tags_url)
        .send()
        .map_err(|e| format!("Ollama unreachable at {url}: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", resp.status()));
    }

    let tags: OllamaTagsResponse = resp
        .json()
        .map_err(|e| format!("Failed to parse /api/tags response: {e}"))?;

    Ok(tags.models)
}

/// Filter models to only those suitable for trading arbitration
/// (prioritizing preferred model families).
pub fn filter_suitable_models(models: &[OllamaModelInfo]) -> Vec<OllamaModelInfo> {
    models
        .iter()
        .filter(|m| {
            let lower = m.name.to_lowercase();
            PREFERRED_MODELS.iter().any(|p| lower.starts_with(p))
        })
        .cloned()
        .collect()
}

/// Group models by family for display (e.g., "Llama 3.2" → "llama3.2:3b, llama3.2:1b")
pub fn group_models_by_family(models: &[OllamaModelInfo]) -> Vec<(String, Vec<String>)> {
    let mut families: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for m in models {
        let family = m.name.split(':').next().unwrap_or(&m.name).to_string();
        families.entry(family).or_default().push(m.name.clone());
    }

    families.into_iter().collect()
}

// ── HF Cache Discovery ──────────────────────────────────────────────────────

/// Check if the Llama GGUF model is cached in HuggingFace Hub.
pub fn is_gguf_cached() -> bool {
    cotrader_ml::models::reasoning_engine::is_model_cached()
}

/// Check if Chronos-Bolt is cached.
pub fn is_chronos_cached() -> bool {
    cotrader_ml::models::chronos_bolt::is_model_cached()
}

// ── Interactive Setup Wizard ─────────────────────────────────────────────────

/// Result of the setup wizard.
pub struct SetupResult {
    pub system_config: cotrader_core::config::SystemConfig,
}

/// Run the full interactive setup wizard.
///
/// Presents the user with available model backends, saves the choice,
/// and returns the resulting SystemConfig.
pub fn run_setup_wizard() -> Result<SetupResult, Box<dyn std::error::Error + Send + Sync>> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║       CoTrader Bootstrap — Model Backend Setup        ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // ── Phase 1: Detect available backends ──────────────────────────
    println!("[1/3] Scanning for available backends...\n");

    // Check Ollama
    let ollama_url = "http://localhost:11434";
    let ollama_models = match discover_ollama_models(ollama_url) {
        Ok(models) => {
            let suitable = filter_suitable_models(&models);
            if suitable.is_empty() {
                if models.is_empty() {
                    println!("  ✓ Ollama running at {ollama_url} — no models installed.");
                    println!("    (run `ollama pull llama3.2:3b` to install a compatible model)\n");
                } else {
                    let grouped = group_models_by_family(&models);
                    let names: Vec<String> = grouped
                        .iter()
                        .flat_map(|(_, ms)| ms.clone())
                        .collect();
                    println!("  ✓ Ollama running at {ollama_url}");
                    println!("    Installed models: {}", names.join(", "));
                    println!("    (none match preferred families: {})\n", PREFERRED_MODELS.join(", "));
                }
                None
            } else {
                let grouped = group_models_by_family(&suitable);
                println!("  ✓ Ollama running at {ollama_url}");
                for (family, models) in &grouped {
                    println!("    {family}: {}", models.join(", "));
                }
                println!();
                Some(suitable)
            }
        }
        Err(e) => {
            println!("  ✗ Ollama not detected: {e}\n");
            None
        }
    };

    // Check HF cache (GGUF via Candle)
    let gguf_cached = is_gguf_cached();

    if gguf_cached {
        println!("  ✓ GGUF (Candle): Llama-3.2-3B cached (~2GB RAM)");
    } else {
        println!("  ✗ GGUF (Candle): Llama-3.2-3B not cached (run `cotrader download-llm`)");
    }

    if is_chronos_cached() {
        println!("  ✓ Chronos-Bolt: cached (~820MB RAM)");
    } else {
        println!("  ✗ Chronos-Bolt: not cached (run `cotrader download`)");
    }
    println!();

    // ── Phase 2: Build option list ──────────────────────────────────
    println!("[2/3] Select LLM backend for signal arbitration:");
    println!();

    let mut options: Vec<(String, cotrader_core::config::LlamaBackend)> = Vec::new();

    // Option: Skip LLM entirely
    options.push((
        "None — consensus-only (no LLM arbitration, zero additional RAM)".to_string(),
        cotrader_core::config::LlamaBackend::None,
    ));

    // Option: Ollama (if models found)
    if let Some(models) = &ollama_models {
        // Pick the largest suitable model (assuming it's the most capable)
        let best = models.first().cloned().unwrap();
        options.push((
            format!(
                "Ollama — {} @ {} (zero additional RAM, ~100ms latency)",
                best.name, ollama_url
            ),
            cotrader_core::config::LlamaBackend::Ollama {
                url: ollama_url.to_string(),
                model: best.name.clone(),
            },
        ));

        // If multiple models, add each as a separate option
        if models.len() > 1 {
            for model in models.iter().skip(1) {
                options.push((
                    format!(
                        "Ollama — {} @ {}",
                        model.name, ollama_url
                    ),
                    cotrader_core::config::LlamaBackend::Ollama {
                        url: ollama_url.to_string(),
                        model: model.name.clone(),
                    },
                ));
            }
        }
    }

    // Option: Candle GGUF (if cached)
    if gguf_cached {
        options.push((
            "Candle GGUF — Llama-3.2-3B via Candle (~2GB RAM, ~6s inference)".to_string(),
            cotrader_core::config::LlamaBackend::CandleGGUF,
        ));
    }

    // Display options
    for (i, (desc, _)) in options.iter().enumerate() {
        println!("  {}. {desc}", i + 1);
    }
    println!();

    // ── Phase 3: Get user selection ─────────────────────────────────
    let selection = loop {
        print!("Enter choice [1-{}]: ", options.len());
        stdout.flush()?;

        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;
        let input = input.trim();

        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= options.len() {
                break n - 1;
            }
        }
        println!("  Invalid choice. Please enter a number between 1 and {}.", options.len());
    };

    let (_desc, backend) = options[selection].clone();

    // ── Phase 4: Save configuration ─────────────────────────────────
    println!();
    println!("[3/3] Saving configuration...");

    let system_config = cotrader_core::config::SystemConfig {
        setup_completed: true,
        llama_backend: backend,
    };

    system_config.save()?;
    println!("  ✓ Configuration saved to {}", cotrader_core::config::SystemConfig::path().display());
    println!();

    // ── Summary ─────────────────────────────────────────────────────
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║       Setup Complete                                    ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    let sys = &system_config;
    match &sys.llama_backend {
        cotrader_core::config::LlamaBackend::Ollama { url, model } => {
            println!("  LLM:   Ollama — {model} @ {url}");
            println!("  RAM:   ~0 MB (Ollama runs as separate process)");
            println!("  Speed: ~100ms per arbitration");
        }
        cotrader_core::config::LlamaBackend::CandleGGUF => {
            println!("  LLM:   Candle — Llama-3.2-3B GGUF");
            println!("  RAM:   ~2 GB");
            println!("  Speed: ~6s per arbitration (CPU)");
        }
        cotrader_core::config::LlamaBackend::None => {
            println!("  LLM:   Disabled — consensus-only arbitration");
        }
    }
    println!();

    Ok(SetupResult {
        system_config,
    })
}
