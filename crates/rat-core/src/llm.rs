// ═══════════════════════════════════════════════════════════════════════════════
// LLM REMOVED — All trading decisions are now purely deterministic.
//
// The system trades exclusively via:
//   - HardRulesGate (Layer 1): Priority-based rule enforcement
//   - StrategyDecisionAgent (Layer 2): Kelly Criterion + volatility sizing
//   - DebateLayer (Layer 3): Multi-agent evidence aggregation
//   - SuperIntelligence (Layer 4): Cross-validation + conviction stack
//   - ExecutionEngine (Layer 5): Atomic order settlement
//
// No neural network calls. No Ollama. No API keys. Pure code.
// ═══════════════════════════════════════════════════════════════════════════════

/// Stub LLM client — kept for type compatibility but performs no operations.
pub struct LlmClient;

impl LlmClient {
    pub fn new() -> Self {
        Self
    }

    pub fn get_model(&self) -> &str {
        "none (rules-only mode)"
    }

    pub async fn is_ollama_running(&self) -> bool {
        false
    }
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Stub executor — kept for type compatibility.
#[derive(Debug, Clone)]
pub struct LlmExecutor {
    pub endpoint: String,
}

impl LlmExecutor {
    pub fn new() -> Self {
        Self { endpoint: String::new() }
    }

    pub fn from_config(_config: &crate::config::Config) -> Self {
        Self { endpoint: String::new() }
    }

    pub fn get_model(&self) -> &str {
        "none (rules-only mode)"
    }

    pub async fn is_ollama_running(&self) -> bool {
        false
    }

    pub async fn embed_text(&self, _text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![0.0; 384])
    }

    pub async fn summarize_news(&self, _headlines: &[String], _symbol: &str) -> String {
        String::new()
    }

    pub async fn ask_for_trade_decision(&self, _request: crate::messages::LLMRequest) -> Result<LlmTradeDecision, Box<dyn std::error::Error + Send + Sync>> {
        Ok(LlmTradeDecision::default())
    }

    pub async fn ask_for_reflection(&self, _prompt: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(String::new())
    }
}

/// Stub trade decision — kept for type compatibility.
#[derive(Debug, Clone, Default)]
pub struct LlmTradeDecision {
    pub action: String,
    pub confidence: f64,
    pub reason: String,
    pub entry: f64,
    pub sl: f64,
    pub tp: f64,
}
