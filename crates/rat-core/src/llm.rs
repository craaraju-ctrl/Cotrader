// ═══════════════════════════════════════════════════════════════════════════════
// LLM Layer — Real Ollama integration with deterministic fallback.
//
// When Ollama is running: uses real LLM for trade decisions, reflections, embeddings.
// When Ollama is down: falls back to deterministic rules (no panic, no forced default).
// ═══════════════════════════════════════════════════════════════════════════════

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
        let model = std::env::var("OLLAMA_MODEL")
            .unwrap_or_else(|_| "llama3.2:3b".into());

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

    /// Probe Ollama health endpoint.
    pub async fn probe(&mut self) -> bool {
        let url = format!("{}/api/tags", self.endpoint);
        match reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                self.available = true;
                println!("[LLM] Ollama detected at {} — model: {}", self.endpoint, self.model);
                true
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
        let model = std::env::var("OLLAMA_MODEL")
            .unwrap_or_else(|_| "llama3.2:3b".into());

        Self { endpoint, model, available: false }
    }

    pub fn from_config(config: &crate::config::Config) -> Self {
        let endpoint = std::env::var("OLLAMA_BASE_URL").ok()
            .filter(|s| !s.is_empty())
            .or_else(|| if config.llm_endpoint.is_empty() { None } else { Some(config.llm_endpoint.clone()) })
            .unwrap_or_else(|| "http://localhost:11434".into());
        let model = std::env::var("OLLAMA_MODEL").ok()
            .filter(|s| !s.is_empty())
            .or_else(|| if config.llm_model.is_empty() { None } else { Some(config.llm_model.clone()) })
            .unwrap_or_else(|| "llama3.2:3b".into());

        Self { endpoint, model, available: false }
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    pub async fn is_ollama_running(&self) -> bool {
        self.available
    }

    // ── Real LLM Calls ──────────────────────────────────────────────

    /// Generate embedding via Ollama embedding endpoint.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
        if !self.available {
            return Ok(vec![0.0; 384]);
        }

        let url = format!("{}/api/embed", self.endpoint);
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });

        let resp = reqwest::Client::new()
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        let data: serde_json::Value = resp.json().await?;
        let embeddings = data["embeddings"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|emb| emb.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
            .unwrap_or_else(|| vec![0.0; 384]);

        Ok(embeddings)
    }

    /// Summarize news headlines via LLM.
    pub async fn summarize_news(&self, headlines: &[String], symbol: &str) -> String {
        if !self.available || headlines.is_empty() {
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
        if !self.available {
            // Deterministic fallback — analyze the prompt for keywords
            return Ok(self.deterministic_decision(&request.prompt));
        }

        let response = self.chat(&request.prompt, request.temperature, request.max_tokens).await?;

        // Parse LLM response into structured decision
        let action = if response.to_uppercase().contains("BUY") || response.to_uppercase().contains("LONG") {
            "BUY".to_string()
        } else if response.to_uppercase().contains("SELL") || response.to_uppercase().contains("SHORT") {
            "SELL".to_string()
        } else {
            "HOLD".to_string()
        };

        // Extract confidence from response
        let confidence = response.lines()
            .find(|l| l.to_lowercase().contains("confidence"))
            .and_then(|l| l.split_whitespace().last())
            .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
            .map(|c| c / 100.0)
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
        if !self.available {
            return Ok(self.deterministic_reflection(prompt));
        }

        self.chat(prompt, 0.5, 300).await
    }

    // ── Chat Engine ─────────────────────────────────────────────────

    /// Send a chat completion request to Ollama.
    async fn chat(&self, prompt: &str, temperature: f32, max_tokens: u32) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/generate", self.endpoint);
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": temperature,
                "num_predict": max_tokens,
            }
        });

        let resp = reqwest::Client::new()
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?;

        let data: serde_json::Value = resp.json().await?;
        let response = data["response"]
            .as_str()
            .unwrap_or("No response from LLM")
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
