// ═══════════════════════════════════════════════════════════════════════════════
// LLM Layer — Real Ollama integration with deterministic fallback.
//
// When Ollama is running: uses real LLM for trade decisions, reflections, embeddings.
// When Ollama is down: falls back to deterministic rules (no panic, no forced default).
// ═══════════════════════════════════════════════════════════════════════════════

/// Resolve the CHAT model name.
///
/// FIXED (LLM loading audit): both LLM clients read `OLLAMA_MODEL`, which
/// .env sets to `nomic-embed-text` — an EMBEDDING-only model that cannot
/// generate text. Every chat call silently returned nothing ("LLM never
/// used"). Priority is now: LLM_MODEL → OLLAMA_MODEL (only if it isn't an
/// embedding model) → default.
pub fn resolve_chat_model() -> String {
    if let Ok(m) = std::env::var("LLM_MODEL") {
        if !m.trim().is_empty() {
            return m.trim().to_string();
        }
    }
    if let Ok(m) = std::env::var("OLLAMA_MODEL") {
        let mt = m.trim();
        if !mt.is_empty() {
            if mt.contains("embed") {
                eprintln!(
                    "[LLM] OLLAMA_MODEL={} is an embedding model — ignoring for chat. Set LLM_MODEL for text generation.",
                    mt
                );
            } else {
                return mt.to_string();
            }
        }
    }
    "llama3.2:3b".to_string()
}

/// LLM client that checks Ollama availability at startup.
pub struct LlmClient {
    endpoint: String,
    model: String,
    available: bool,
}

impl LlmClient {
    pub fn new() -> Self {
        let endpoint = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".into());
        let model = resolve_chat_model();

        Self { endpoint, model, available: false }
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    pub fn set_available(&mut self, avail: bool) {
        self.available = avail;
    }

    pub async fn is_ollama_running(&self) -> bool {
        self.available
    }

    /// Probe Ollama health endpoint AND verify the chat model is pulled.
    /// A reachable server with a missing model still fails every generate
    /// call — the old probe reported "available" in that state.
    pub async fn probe(&mut self) -> bool {
        let url = format!("{}/api/tags", self.endpoint);
        match reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                // Compare base names (before ':') so "model" matches "model:latest".
                let want_base = self.model.split(':').next().unwrap_or(&self.model).to_string();
                let model_present = resp
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|j| {
                        j["models"].as_array().map(|arr| {
                            arr.iter().any(|m| {
                                m["name"]
                                    .as_str()
                                    .map(|n| {
                                        n == self.model
                                            || n.split(':').next().unwrap_or(n) == want_base
                                    })
                                    .unwrap_or(false)
                            })
                        })
                    })
                    .unwrap_or(true); // tags unparseable → don't false-negative

                if model_present {
                    self.available = true;
                    println!("[LLM] Ollama detected at {} — model: {}", self.endpoint, self.model);
                    true
                } else {
                    self.available = false;
                    eprintln!(
                        "[LLM] Ollama is running but model '{}' is NOT pulled — run: ollama pull {} (deterministic fallback active)",
                        self.model, self.model
                    );
                    false
                }
            }
            _ => {
                eprintln!("[LLM] Ollama not available at {} — falling back to deterministic rules", self.endpoint);
                self.available = false;
                false
            }
        }
    }
}

impl Default for LlmClient {
    fn default() -> Self { Self::new() }
}

/// LLM executor with real Ollama integration and deterministic fallback.
#[derive(Debug, Clone)]
pub struct LlmExecutor {
    pub endpoint: String,
    pub model: String,
    pub available: bool,
}

impl LlmExecutor {
    pub fn new() -> Self {
        let endpoint = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".into());
        let model = resolve_chat_model();

        Self { endpoint, model, available: false }
    }

    pub fn from_config(config: &crate::config::Config) -> Self {
        let endpoint = std::env::var("OLLAMA_BASE_URL").ok()
            .filter(|s| !s.is_empty())
            .or_else(|| if config.llm_endpoint.is_empty() { None } else { Some(config.llm_endpoint.clone()) })
            .unwrap_or_else(|| "http://localhost:11434".into());
        // LLM_MODEL env (via resolve_chat_model) wins; config.llm_model (which
        // reads LLM_MODEL at config-build time) is the fallback. OLLAMA_MODEL
        // is never used for chat unless it's a non-embedding model.
        let model = match std::env::var("LLM_MODEL").ok().filter(|s| !s.trim().is_empty()) {
            Some(m) => m.trim().to_string(),
            None if !config.llm_model.is_empty() => config.llm_model.clone(),
            None => resolve_chat_model(),
        };

        Self { endpoint, model, available: false }
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    /// Check if Ollama is reachable — probes the server each time.
    /// This replaces the cached `available` flag which was never set to true.
    pub async fn is_ollama_running(&self) -> bool {
        let url = format!("{}/api/tags", self.endpoint);
        reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    // ── Real LLM Calls ──────────────────────────────────────────────

    /// Generate embedding via Ollama embedding endpoint.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_ollama_running().await {
            // Err, not a zero vector: fallbacks are for decisions, never data.
            return Err("Ollama not reachable — cannot embed".into());
        }

        let url = format!("{}/api/embed", self.endpoint);
        // Embeddings must use the EMBEDDING model, never self.model (which is
        // now the chat model after the LLM_MODEL/OLLAMA_MODEL fix).
        let embed_model = std::env::var("OLLAMA_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "nomic-embed-text".into());
        let body = serde_json::json!({
            "model": embed_model,
            "input": text,
        });

        let resp = reqwest::Client::new()
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        let data: serde_json::Value = resp.json().await?;
        // This fn returns Result — a parse failure must be an Err, not a
        // fake zero vector (the old 384-dim zero fallback was both the wrong
        // dimension AND poisonous to cosine similarity).
        let embeddings: Vec<f32> = data["embeddings"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|emb| emb.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
            .ok_or_else(|| format!("Unexpected Ollama embed response: {}", data))?;

        Ok(embeddings)
    }

    /// Summarize news headlines via LLM.
    pub async fn summarize_news(&self, headlines: &[String], symbol: &str) -> String {
        if !self.is_ollama_running().await || headlines.is_empty() {
            return format!("{}: {} headlines analyzed (deterministic summary)", symbol, headlines.len());
        }

        let prompt = format!(
            "Summarize these {} news headlines for {} in 2-3 sentences. \
             Focus on sentiment (bullish/bearish/neutral) and key catalysts:\n\n{}",
            headlines.len(),
            symbol,
            headlines.iter().take(10).enumerate()
                .map(|(i, h)| format!("{}. {}", i + 1, h))
                .collect::<Vec<_>>()
                .join("\n")
        );

        match self.chat(&prompt, 0.3, 200).await {
            Ok(summary) => summary,
            Err(_) => format!("{}: {} headlines — LLM unavailable for summarization", symbol, headlines.len()),
        }
    }

    /// Ask for a trade decision from the LLM.
    pub async fn ask_for_trade_decision(
        &self,
        request: crate::messages::LLMRequest,
    ) -> Result<LlmTradeDecision, Box<dyn std::error::Error + Send + Sync>> {
        // Probe Ollama live instead of relying on cached `available` flag
        if !self.is_ollama_running().await {
            println!("[LLM] Ollama not reachable — using deterministic fallback");
            return Ok(self.deterministic_decision(&request.prompt));
        }

        println!("[LLM] Calling Ollama {} with prompt ({} chars)...", self.model, request.prompt.len());
        let response = match self.chat(&request.prompt, request.temperature, request.max_tokens).await {
            Ok(r) => {
                println!("[LLM] Response ({} chars): {}", r.len(), &r[..r.len().min(200)]);
                r
            }
            Err(e) => {
                println!("[LLM] Chat FAILED: {} — using deterministic fallback", e);
                return Ok(self.deterministic_decision(&request.prompt));
            }
        };

        // Parse LLM response into structured decision
        // Check for BUY/SELL/HOLD keywords in various forms
        let upper = response.to_uppercase();
        let action = if upper.contains("BUY") || upper.contains("LONG") || upper.contains("PURCHASE") {
            "BUY".to_string()
        } else if upper.contains("SELL") || upper.contains("SHORT") || upper.contains("EXIT") {
            "SELL".to_string()
        } else {
            "HOLD".to_string()
        };

        // Extract confidence from response — look for percentage or number
        let confidence = response.lines()
            .find(|l| l.to_lowercase().contains("confidence") || l.contains("%"))
            .and_then(|l| {
                // Try to find a number in the line
                l.split_whitespace()
                    .find(|w| w.parse::<f64>().is_ok() || w.trim_end_matches('%').parse::<f64>().is_ok())
                    .and_then(|w| w.trim_end_matches('%').parse::<f64>().ok())
            })
            .map(|c| if c > 1.0 { c / 100.0 } else { c })
            .unwrap_or(0.5);

        Ok(LlmTradeDecision {
            action,
            confidence,
            reason: response,
            entry: 0.0,
            sl: 0.0,
            tp: 0.0,
        })
    }

    /// Ask for reflection on a trade outcome.
    pub async fn ask_for_reflection(
        &self,
        prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_ollama_running().await {
            return Ok(self.deterministic_reflection(prompt));
        }

        self.chat(prompt, 0.5, 300).await
    }

    // ── Chat Engine ─────────────────────────────────────────────────

    /// Send a chat completion request to Ollama.
    /// Handles thinking models (nemotron, qwen3) that put output in `thinking` field.
    async fn chat(&self, prompt: &str, temperature: f32, max_tokens: u32) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/generate", self.endpoint);
        // Give thinking models extra tokens: they need tokens for thinking AND response
        let adjusted_max = if max_tokens < 200 { max_tokens * 4 } else { max_tokens };
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": temperature,
                "num_predict": adjusted_max,
            }
        });

        let resp = reqwest::Client::new()
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(45))
            .send()
            .await?;

        let data: serde_json::Value = resp.json().await?;

        // Check `response` first (normal models), then `thinking` (thinking models)
        let response = data["response"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| data["thinking"].as_str().filter(|s| !s.is_empty()))
            .unwrap_or("HOLD")
            .to_string();

        Ok(response)
    }

    // ── Deterministic Fallbacks ─────────────────────────────────────

    /// Rule-based fallback when LLM is unavailable.
    fn deterministic_decision(&self, prompt: &str) -> LlmTradeDecision {
        let upper = prompt.to_uppercase();

        let (action, confidence, reason) = if upper.contains("RSI") && upper.contains("OVERSOLD") {
            ("BUY".into(), 0.65, "RSI indicates oversold — deterministic BUY signal".into())
        } else if upper.contains("RSI") && upper.contains("OVERBOUGHT") {
            ("SELL".into(), 0.65, "RSI indicates overbought — deterministic SELL signal".into())
        } else if upper.contains("STRONG") && upper.contains("TREND") {
            ("BUY".into(), 0.7, "Strong uptrend detected — follow trend".into())
        } else if upper.contains("DRAWDOWN") && upper.contains("EXCEEDS") {
            ("SELL".into(), 0.8, "Drawdown exceeds threshold — risk reduction".into())
        } else {
            ("HOLD".into(), 0.4, "Insufficient signal strength — hold position".into())
        };

        LlmTradeDecision { action, confidence, reason, entry: 0.0, sl: 0.0, tp: 0.0 }
    }

    /// Deterministic reflection when LLM is unavailable.
    fn deterministic_reflection(&self, prompt: &str) -> String {
        let upper = prompt.to_uppercase();
        if upper.contains("LOSS") || upper.contains("FAIL") {
            "Reflection: Trade resulted in loss. Review entry criteria and stop placement. \
             Consider if the setup was valid or if market conditions changed."
                .to_string()
        } else if upper.contains("WIN") || upper.contains("SUCCESS") {
            "Reflection: Trade was profitable. Confirm the edge was captured correctly. \
             Note what worked for future reference."
                .to_string()
        } else {
            "Reflection: Outcome pending. Continue monitoring and update assessment."
                .to_string()
        }
    }
}

/// Trade decision from LLM or deterministic rules.
#[derive(Debug, Clone)]
pub struct LlmTradeDecision {
    pub action: String,
    pub confidence: f64,
    pub reason: String,
    pub entry: f64,
    pub sl: f64,
    pub tp: f64,
}

impl Default for LlmTradeDecision {
    fn default() -> Self {
        Self {
            action: "HOLD".into(),
            confidence: 0.0,
            reason: "No decision made".into(),
            entry: 0.0,
            sl: 0.0,
            tp: 0.0,
        }
    }
}
