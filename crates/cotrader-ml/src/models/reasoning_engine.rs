//! Reasoning Engine — Llama-3.2-3B for complex signal arbitration.
//!
//! Implements the LLM escalation gate from the architecture:
//!   [Tri-Level Validator] → conflict/high-risk detected → LLM fires → FinalSignal
//!                                                        ↙ no conflict → skip LLM
//!
//! Uses a quantized GGUF model loaded via candle-transformers.
//! The LLM is NOT called on every pipeline cycle — only when:
//!   1. Direction conflict between layers (e.g. Rules says BUY, Chronos says SELL)
//!   2. High volatility regime (ATR-based or regime-classifier flag)
//!   3. Low-confidence consensus despite strong individual signals
//!
//! Model: bartowski/Llama-3.2-3B-Instruct-GGUF (Q4_K_M, ~2GB)

use candle_core::{Device, Tensor};
use candle_core::quantized::gguf_file;
use candle_transformers::models::quantized_llama::ModelWeights as QuantizedLlama;
use std::io::BufReader;
use std::path::Path;

// ── Constants ────────────────────────────────────────────────────────────────

/// HuggingFace repo for the quantized Llama model (GGUF weights only).
pub const LLM_MODEL_REPO: &str = "bartowski/Llama-3.2-3B-Instruct-GGUF";
/// GGUF file to download.
pub const LLM_MODEL_FILE: &str = "Llama-3.2-3B-Instruct-Q4_K_M.gguf";
/// HuggingFace repo for the tokenizer (bartowski repos don't include it).
/// Using unsloth mirror to avoid gated access on meta-llama repos.
pub const LLM_TOKENIZER_REPO: &str = "unsloth/Llama-3.2-3B-Instruct";

/// Production mode constants — fast, minimal output.
pub const PRODUCTION_MAX_GEN_TOKENS: usize = 128;
pub const PRODUCTION_TEMPERATURE: f64 = 0.3;
pub const PRODUCTION_TOP_P: f64 = 0.9;

/// Inspection mode constants — verbose chain-of-thought, extended reasoning.
pub const INSPECTION_MAX_GEN_TOKENS: usize = 2048;
pub const INSPECTION_TEMPERATURE: f64 = 0.7;
pub const INSPECTION_TOP_P: f64 = 0.95;

/// Get generation parameters based on system mode.
pub fn get_generation_params(is_inspection: bool) -> (usize, f64, f64) {
    if is_inspection {
        (INSPECTION_MAX_GEN_TOKENS, INSPECTION_TEMPERATURE, INSPECTION_TOP_P)
    } else {
        (PRODUCTION_MAX_GEN_TOKENS, PRODUCTION_TEMPERATURE, PRODUCTION_TOP_P)
    }
}

// ── Arbitration Types ────────────────────────────────────────────────────────

/// Input to the arbitration function — all layer signals at pipeline time.
#[derive(Debug, Clone)]
pub struct ArbitrationInput {
    // Rules Layer
    pub rules_action: String,     // "BUY" / "SELL" / "HOLD"
    pub rules_confidence: f64,    // 0.0–1.0
    pub rules_signal: f64,        // -1.0–+1.0
    // ML Layer
    pub ml_action: String,
    pub ml_confidence: f64,
    pub ml_signal: f64,
    // Chronos-Bolt Forecasting
    pub chronos_action: String,
    pub chronos_confidence: f64,
    pub chronos_signal: f64,
    // Market Context
    pub market_regime: String,    // "TrendingBull" / "TrendingBear" / "Ranging" / "Volatile"
    pub volatility_ratio: f64,    // Current ATR / 20-day avg ATR
    pub symbol: String,
    pub current_price: f64,
    // CNN Pattern Detection (from pattern_detector.rs)
    pub pattern_detected: String, // e.g. "StrongBullish" / "WeakBearish" / "None"
    pub pattern_confidence: f64,
    // Sentiment Analysis (from FinBERT pipeline)
    pub sentiment_score: f64,     // -1.0 (hyper-bearish) to +1.0 (hyper-bullish)
    pub sentiment_confidence: f64, // 0.0–1.0
    // Vector Memory Context (from agentic memory server)
    pub vector_memory_context: String,
    // News Context (from news fetcher)
    pub news_context: String,
    // Multi-Timeframe Analysis
    pub multi_tf_context: String,
    // Agent Market Summary
    pub agent_summary: String,
}

impl std::fmt::Display for ArbitrationInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Symbol: {} @ ${:.2}\n\
             Rules Layer: {} (signal={:+.3}, conf={:.2})\n\
             ML Layer:    {} (signal={:+.3}, conf={:.2})\n\
             Chronos:     {} (signal={:+.3}, conf={:.2})\n\
             Patterns:    {} (conf={:.2})\n\
             Sentiment:   score={:+.3} conf={:.2}\n\
             Market: {} | Volatility Ratio: {:.2}x",
            self.symbol, self.current_price,
            self.rules_action, self.rules_signal, self.rules_confidence,
            self.ml_action, self.ml_signal, self.ml_confidence,
            self.chronos_action, self.chronos_signal, self.chronos_confidence,
            self.pattern_detected, self.pattern_confidence,
            self.sentiment_score, self.sentiment_confidence,
            self.market_regime, self.volatility_ratio,
        )
    }
}

/// Final arbitration result.
#[derive(Debug, Clone)]
pub struct FinalSignal {
    /// "BUY" / "SELL" / "HOLD"
    pub direction: String,
    /// Confidence 0.0–1.0
    pub confidence: f64,
    /// Human-readable reasoning from the LLM or gate fallback.
    pub reasoning: String,
    /// Whether the LLM was actually invoked (vs. gate deciding to skip).
    pub llm_used: bool,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Reasoning Engine
// ═══════════════════════════════════════════════════════════════════════════════

/// Wraps a quantized Llama-3.2-3B model for on-demand signal arbitration.
///
/// The engine checks an **escalation gate** before running inference:
/// - If all layers agree on direction → skip LLM (return consensus directly)
/// - If conflict or high volatility → fire LLM with structured prompt
pub struct ReasoningEngine {
    /// The quantized Llama model.
    model: QuantizedLlama,
    /// Tokenizer loaded from the model cache.
    tokenizer: tokenizers::Tokenizer,
    /// Compute device.
    device: Device,
    /// EOS token ID.
    eos_token_id: u32,
}

impl ReasoningEngine {
    /// Load the model and tokenizer from a cached directory.
    ///
    /// Expects `model_path` to contain:
    ///   - `Llama-3.2-3B-Instruct-Q4_K_M.gguf`
    ///   - `tokenizer.json`
    pub fn load(model_path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let gguf_path = model_path.join(LLM_MODEL_FILE);
        if !gguf_path.exists() {
            return Err(format!(
                "GGUF model not found at {}. Run `cotrader download-llm` first.",
                gguf_path.display()
            )
            .into());
        }

        let tokenizer_path = model_path.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(format!(
                "Tokenizer not found at {}. Run `cotrader download-llm` to fetch it.",
                tokenizer_path.display()
            )
            .into());
        }

        let device = Device::Cpu;

        // ── Load GGUF model ────────────────────────────────────────────
        let mut file = BufReader::new(std::fs::File::open(&gguf_path)?);
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| format!("Failed to parse GGUF header: {e}"))?;
        let model = QuantizedLlama::from_gguf(content, &mut file, &device)
            .map_err(|e| format!("Failed to load quantized Llama: {e}"))?;

        // ── Load tokenizer ─────────────────────────────────────────────
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {e}"))?;

        // Determine EOS token — Llama 3.2 uses <|eot_id|> (token 128009) or </s> (token 2)
        let vocab = tokenizer.get_vocab(true);
        let eos_token_id = *vocab
            .get("<|eot_id|>")
            .or_else(|| vocab.get("</s>"))
            .or_else(|| vocab.get("<|end_of_text|>"))
            .unwrap_or(&2u32);

        Ok(Self {
            model,
            tokenizer,
            device,
            eos_token_id,
        })
    }

    // ── Escalation Gate ─────────────────────────────────────────────────────

    /// Check whether the LLM should be triggered.
    ///
    /// Returns `true` if any of these conditions hold:
    ///   1. **Direction conflict**: Two or more layers disagree (e.g. BUY vs SELL)
    ///   2. **High volatility**: volatility_ratio > 1.5x baseline
    ///   3. **Volatile regime**: market is in "Volatile" regime
    ///   4. **Uniform disagreement**: All three layers give different directions
    pub fn escalation_triggered(input: &ArbitrationInput) -> bool {
        // Extract actions as comparable values
        let rules_num = Self::action_to_num(&input.rules_action);
        let ml_num = Self::action_to_num(&input.ml_action);
        let chronos_num = Self::action_to_num(&input.chronos_action);

        // Count unique non-HOLD directions
        let directions: Vec<i8> = [rules_num, ml_num, chronos_num]
            .iter()
            .filter(|&&d| d != 0) // exclude HOLD
            .copied()
            .collect();

        // Condition 1: Direction conflict — at least one BUY and one SELL
        let has_buy = directions.contains(&1);
        let has_sell = directions.contains(&-1);
        let conflict = has_buy && has_sell;

        if conflict {
            return true;
        }

        // Condition 2: High volatility
        if input.volatility_ratio > 1.5 {
            return true;
        }

        // Condition 3: Volatile regime
        if input.market_regime.eq_ignore_ascii_case("volatile") {
            return true;
        }

        // Condition 4: All three layers active but all disagree (BUY, SELL, HOLD)
        let all_active = input.rules_confidence > 0.3
            && input.ml_confidence > 0.3
            && input.chronos_confidence > 0.3;
        if all_active {
            let unique: std::collections::HashSet<i8> =
                [rules_num, ml_num, chronos_num].iter().copied().collect();
            if unique.len() >= 3 {
                return true;
            }
        }

        false
    }

    fn action_to_num(action: &str) -> i8 {
        match action {
            "BUY" => 1,
            "SELL" => -1,
            _ => 0,
        }
    }

    // ── Build Prompt ────────────────────────────────────────────────────────

    /// Build a concise arbitration prompt for production mode.
    pub fn build_prompt(input: &ArbitrationInput) -> String {
        Self::build_prompt_inner(input, false)
    }

    /// Build a verbose chain-of-thought prompt for inspection mode.
    pub fn build_inspection_prompt(input: &ArbitrationInput) -> String {
        Self::build_prompt_inner(input, true)
    }

    /// Internal prompt builder with inspection mode toggle.
    fn build_prompt_inner(input: &ArbitrationInput, inspection: bool) -> String {
        // Format sentiment score for prompt
        let sentiment_label = if input.sentiment_score < -0.6 {
            "hyper-bearish"
        } else if input.sentiment_score < -0.2 {
            "bearish"
        } else if input.sentiment_score <= 0.2 {
            "neutral"
        } else if input.sentiment_score <= 0.6 {
            "bullish"
        } else {
            "hyper-bullish"
        };

        if inspection {
            // Verbose inspection mode — full chain-of-thought reasoning
            format!(
                "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
                You are an expert trading signal arbitrator performing deep analysis. \
                Your job is to provide COMPREHENSIVE chain-of-thought reasoning that \
                covers ALL aspects of the market situation before making a decision. \
                You MUST analyze each layer systematically and explain your reasoning \
                in detail (500-2000 words). Output structured analysis with clear sections.\n\
                <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
                DEEP ANALYSIS REQUEST for {symbol} @ ${price:.2}\n\n\
                === INPUT SIGNALS ===\n\n\
                Rules Layer:   {rules_act} (signal={rules_sig:+.3}, confidence={rules_conf:.2})\n\
                ML Layer:      {ml_act} (signal={ml_sig:+.3}, confidence={ml_conf:.2})\n\
                Chronos Trend: {chronos_act} (signal={chronos_sig:+.3}, confidence={chronos_conf:.2})\n\
                CNN Pattern:   {patterns} (confidence={pattern_conf:.2})\n\
                Sentiment:     {sent_score:+.3} ({sent_label}, confidence={sent_conf:.2})\n\n\
                === MARKET CONTEXT ===\n\n\
                Market Regime: {regime}\n\
                Volatility:    {vol:.2}x normal\n\
                Cornish-Fisher VaR: 99% confidence threshold\n\n\
                === REQUIRED ANALYSIS SECTIONS ===\n\n\
                1. MACRO-REGIME ANALYSIS:\n\
                   - Current market regime assessment\n\
                   - Regime stability and transition probability\n\
                   - Historical regime context\n\n\
                2. MULTI-TIMEFRAME INDICATOR CONFLUENCE:\n\
                   - Rules layer signal interpretation\n\
                   - ML/Signal layer technical analysis\n\
                   - Chronos-Bolt forecast trajectory\n\
                   - Pattern detection significance\n\n\
                3. SENTIMENT MODIFIER ANALYSIS:\n\
                   - News sentiment score interpretation\n\
                   - Sentiment momentum and divergence\n\
                   - Market psychology assessment\n\n\
                4. RISK ASSESSMENT (Cornish-Fisher VaR):\n\
                   - Skewness and kurtosis implications\n\
                   - Tail risk evaluation\n\
                   - Position sizing recommendation\n\n\
                5. CONFLICT RESOLUTION:\n\
                   - Identify disagreements between layers\n\
                   - Weight evidence for each side\n\
                   - Explain resolution logic\n\n\
                6. FINAL DECISION:\n\
                   DECISION: BUY/SELL/HOLD\n\
                   CONFIDENCE: 0.0-1.0\n\
                   REASONING: (comprehensive explanation)\n\
                <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n",
                symbol = input.symbol,
                price = input.current_price,
                rules_act = input.rules_action,
                rules_sig = input.rules_signal,
                rules_conf = input.rules_confidence,
                ml_act = input.ml_action,
                ml_sig = input.ml_signal,
                ml_conf = input.ml_confidence,
                chronos_act = input.chronos_action,
                chronos_sig = input.chronos_signal,
                chronos_conf = input.chronos_confidence,
                regime = input.market_regime,
                vol = input.volatility_ratio,
                patterns = input.pattern_detected,
                pattern_conf = input.pattern_confidence,
                sent_score = input.sentiment_score,
                sent_label = sentiment_label,
                sent_conf = input.sentiment_confidence,
            )
        } else {
            // Production mode — concise output with context
            format!(
                "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
                You are a trading signal arbitrator. Your job is to reconcile conflicting \
                signals from four independent analysis layers. Output ONLY a structured \
                decision with brief reasoning.\n\
                <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
                Reconcile these signals for {symbol} @ ${price:.2}:\n\n\
                Rules Layer:   {rules_act} (sig={rules_sig:+.3}, conf={rules_conf:.2})\n\
                ML Layer:      {ml_act} (sig={ml_sig:+.3}, conf={ml_conf:.2})\n\
                Chronos Trend: {chronos_act} (sig={chronos_sig:+.3}, conf={chronos_conf:.2})\n\
                CNN Pattern:   {patterns} (conf={pattern_conf:.2})\n\
                Sentiment:     {sent_score:+.3} ({sent_label}, conf={sent_conf:.2})\n\
                Market Regime: {regime}\n\
                Volatility:    {vol:.2}x normal\n\n\
                Vector Memory: {vector}\n\
                News Context: {news}\n\
                Multi-TF Analysis: {mtf}\n\n\
                Output exactly:\n\
                DECISION: BUY/SELL/HOLD\n\
                CONFIDENCE: 0.0-1.0\n\
                REASONING: (one sentence)\n\
                <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n",
                symbol = input.symbol,
                price = input.current_price,
                rules_act = input.rules_action,
                rules_sig = input.rules_signal,
                rules_conf = input.rules_confidence,
                ml_act = input.ml_action,
                ml_sig = input.ml_signal,
                ml_conf = input.ml_confidence,
                chronos_act = input.chronos_action,
                chronos_sig = input.chronos_signal,
                chronos_conf = input.chronos_confidence,
                regime = input.market_regime,
                vol = input.volatility_ratio,
                patterns = input.pattern_detected,
                pattern_conf = input.pattern_confidence,
                sent_score = input.sentiment_score,
                sent_label = sentiment_label,
                sent_conf = input.sentiment_confidence,
                vector = input.vector_memory_context,
                news = input.news_context,
                mtf = input.multi_tf_context,
            )
        }
    }

    // ── Inference ───────────────────────────────────────────────────────────

    /// Run the LLM on the prompt with configurable generation parameters.
    fn run_llm_with_params(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        temperature: f64,
        top_p: f64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Encode
        let encoding = self.tokenizer
            .encode(prompt, true)
            .map_err(|e| format!("Tokenization failed: {e}"))?;
        let tokens = encoding.get_ids().to_vec();
        let prompt_len = tokens.len();

        // Early exit for empty prompt
        if tokens.is_empty() {
            return Ok(String::new());
        }

        let mut output_tokens: Vec<u32> = Vec::with_capacity(max_tokens);
        let mut next_token = tokens[0];

        // ── Prefill: process prompt tokens ─────────────────────────────
        for pos in 0..prompt_len {
            let input = Tensor::new(&[next_token as i64], &self.device)?
                .unsqueeze(0)?;
            let _logits = self.model.forward(&input, pos)?;
            if pos + 1 < prompt_len {
                next_token = tokens[pos + 1];
            }
        }

        // ── Generation: auto-regressive decode ─────────────────────────
        let gen_start = prompt_len;
        for pos in 0..max_tokens {
            let input = Tensor::new(&[next_token as i64], &self.device)?
                .unsqueeze(0)?;
            let logits = self.model.forward(&input, gen_start + pos)?;

            // Sample next token with configurable parameters
            next_token = Self::sample_token_with_params(&logits, temperature, top_p)?;

            if next_token == self.eos_token_id || next_token == 0 {
                break;
            }
            output_tokens.push(next_token);
        }

        // Decode
        let output = self.tokenizer
            .decode(&output_tokens, true)
            .map_err(|e| format!("Decoding failed: {e}"))?;

        Ok(output.trim().to_string())
    }

    /// Run the LLM with production parameters (backward compatible).
    fn run_llm(&mut self, prompt: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.run_llm_with_params(prompt, PRODUCTION_MAX_GEN_TOKENS, PRODUCTION_TEMPERATURE, PRODUCTION_TOP_P)
    }

    /// Sample from logits using top-p (nucleus) sampling with temperature.
    fn sample_token_with_params(
        logits: &Tensor,
        temperature: f64,
        top_p: f64,
    ) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        let logits = logits.squeeze(0)?; // remove batch dim

        // Apply temperature
        let logits = if (temperature - 1.0).abs() > 1e-6 {
            (&logits / temperature)?
        } else {
            logits
        };

        // Softmax to get probabilities
        let probs = candle_nn::ops::softmax(&logits, 0)?;
        let probs_vec: Vec<f32> = probs.to_vec1()?;

        // Top-p (nucleus) sampling
        let mut indexed: Vec<(usize, f32)> = probs_vec.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut cumsum = 0.0f32;
        let mut candidate_idx = indexed[0].0;
        for (idx, prob) in &indexed {
            cumsum += prob;
            if cumsum >= top_p as f32 {
                candidate_idx = *idx;
                break;
            }
        }

        // If top-p gave us nothing useful, take argmax
        if cumsum < 0.01 {
            candidate_idx = indexed[0].0;
        }

        Ok(candidate_idx as u32)
    }

    /// Sample from logits using default production parameters.
    fn sample_token(logits: &Tensor) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        Self::sample_token_with_params(logits, PRODUCTION_TEMPERATURE, PRODUCTION_TOP_P)
    }

    // ── Parse Response ──────────────────────────────────────────────────────

    /// Parse the LLM output into a structured FinalSignal.
    pub fn parse_response(response: &str, default_signal: &FinalSignal) -> FinalSignal {
        let response_upper = response.to_uppercase();

        // Extract direction
        let direction = if response_upper.contains("DECISION: BUY")
            || response_upper.contains("BUY")
        {
            "BUY"
        } else if response_upper.contains("DECISION: SELL")
            || response_upper.contains("SELL")
        {
            "SELL"
        } else {
            "HOLD"
        };

        // Extract confidence
        let confidence = if let Some(line) = response.lines().find(|l| l.to_uppercase().contains("CONFIDENCE")) {
            // Find a number in the line
            line.split(|c: char| !c.is_ascii_digit() && c != '.')
                .filter_map(|s| s.parse::<f64>().ok())
                .next()
                .map(|v| v.clamp(0.0, 1.0))
                .unwrap_or(default_signal.confidence)
        } else {
            default_signal.confidence
        };

        // Extract reasoning
        let reasoning = response.lines()
            .find(|l| l.to_uppercase().contains("REASONING"))
            .map(|l| l.trim_start_matches(|c: char| c.is_uppercase() || c == ':' || c == ' '))
            .unwrap_or("LLM arbitration completed.")
            .to_string();

        FinalSignal {
            direction: direction.to_string(),
            confidence,
            reasoning,
            llm_used: true,
        }
    }

    // ── Public Arbitration API ──────────────────────────────────────────────

    /// The main entry point for signal arbitration.
    ///
    /// 1. Runs the escalation gate (`escalation_triggered`)
    /// 2. If gate fires → builds prompt → runs LLM → parses response
    /// 3. If gate does NOT fire → returns a consensus-based signal with `llm_used=false`
    pub fn arbitrate(&mut self, input: &ArbitrationInput) -> Result<FinalSignal, Box<dyn std::error::Error + Send + Sync>> {
        self.arbitrate_with_mode(input, false)
    }

    /// Arbitrate with inspection mode toggle for verbose chain-of-thought.
    pub fn arbitrate_with_mode(
        &mut self,
        input: &ArbitrationInput,
        inspection_mode: bool,
    ) -> Result<FinalSignal, Box<dyn std::error::Error + Send + Sync>> {
        // ── Escalation Gate ────────────────────────────────────────────
        if !Self::escalation_triggered(input) {
            // No conflict — return weighted consensus without LLM
            let consensus = Self::compute_consensus(input);
            return Ok(FinalSignal {
                llm_used: false,
                ..consensus
            });
        }

        // ── Build prompt based on mode ─────────────────────────────────
        let prompt = if inspection_mode {
            Self::build_inspection_prompt(input)
        } else {
            Self::build_prompt(input)
        };

        // ── Run LLM with appropriate parameters ───────────────────────
        let (max_tokens, temperature, top_p) = get_generation_params(inspection_mode);
        let response = self.run_llm_with_params(&prompt, max_tokens, temperature, top_p)?;

        // Create default consensus as fallback
        let default = Self::compute_consensus(input);

        // Parse structured output
        let parsed = Self::parse_response(&response, &default);

        // Log the arbitration with mode indicator
        let mode_label = if inspection_mode { "INSPECTION" } else { "PRODUCTION" };
        println!(
            "[LLM-Arb] [{}] {} → {} (conf={:.2}, llm_used=true) | {}",
            mode_label, input.symbol, parsed.direction, parsed.confidence, parsed.reasoning
        );

        // In inspection mode, also emit the full reasoning to stderr for CLI telemetry
        if inspection_mode {
            eprintln!("[LLM-REASONING] ═══════════════════════════════════════════════════════════════");
            eprintln!("[LLM-REASONING] {} | Full Chain-of-Thought Output:", input.symbol);
            eprintln!("[LLM-REASONING] ═══════════════════════════════════════════════════════════════");
            for line in response.lines() {
                eprintln!("[LLM-REASONING] {}", line);
            }
            eprintln!("[LLM-REASONING] ═══════════════════════════════════════════════════════════════");
        }

        Ok(parsed)
    }

    /// Compute a consensus signal from the three layers (no LLM).
    pub fn compute_consensus(input: &ArbitrationInput) -> FinalSignal {
        // Weighted average of signals (including sentiment as 4th factor)
        // Sentiment gets lower weight since it's supplementary to technical signals
        let weights = [0.35, 0.25, 0.25, 0.15]; // rules, ml, chronos, sentiment
        let signals = [
            input.rules_signal,
            input.ml_signal,
            input.chronos_signal,
            input.sentiment_score,
        ];
        let confidences = [
            input.rules_confidence,
            input.ml_confidence,
            input.chronos_confidence,
            input.sentiment_confidence,
        ];

        let weighted_signal: f64 = signals.iter().zip(weights.iter()).map(|(s, w)| s * w).sum();
        let avg_confidence: f64 = confidences.iter().zip(weights.iter()).map(|(c, w)| c * w).sum();

        let direction = if weighted_signal > 0.15 {
            "BUY"
        } else if weighted_signal < -0.15 {
            "SELL"
        } else {
            "HOLD"
        };

        FinalSignal {
            direction: direction.to_string(),
            confidence: avg_confidence.clamp(0.0, 1.0),
            reasoning: format!(
                "Consensus (LLM skipped): rules={:.2} ml={:.2} chronos={:.2} sentiment={:+.2} → {}",
                input.rules_signal, input.ml_signal, input.chronos_signal, input.sentiment_score, direction
            ),
            llm_used: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Model Management (download, cache, auto-load helpers)
// ═══════════════════════════════════════════════════════════════════════════════

/// Download the Llama model (GGUF) and tokenizer from separate HuggingFace repos.
///
/// The GGUF weights come from `bartowski/Llama-3.2-3B-Instruct-GGUF`.
/// The tokenizer comes from `unsloth/Llama-3.2-3B-Instruct` (to avoid gated access).
/// The tokenizer is copied into the GGUF cache directory so `load()` can find both.
pub fn download_model() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api = hf_hub::api::sync::Api::new()?;

    // 1. Download GGUF weights from bartowski repo
    let gguf_repo = api.model(LLM_MODEL_REPO.to_string());
    println!("[LLM] Downloading {} from {}...", LLM_MODEL_FILE, LLM_MODEL_REPO);
    let gguf_path = gguf_repo.get(LLM_MODEL_FILE)?;

    // 2. Download tokenizer from unsloth repo (same tokenizer, no auth required)
    let tok_repo = api.model(LLM_TOKENIZER_REPO.to_string());
    println!("[LLM] Downloading tokenizer.json from {}...", LLM_TOKENIZER_REPO);
    let tok_path = tok_repo.get("tokenizer.json")?;

    // 3. Copy tokenizer into GGUF cache dir so both files are co-located
    let gguf_dir = gguf_path.parent().unwrap_or(std::path::Path::new("."));
    let target_tok_path = gguf_dir.join("tokenizer.json");
    if !target_tok_path.exists() {
        std::fs::copy(&tok_path, &target_tok_path)?;
        println!("[LLM] Copied tokenizer.json to {}", target_tok_path.display());
    }

    println!(
        "[LLM] Model cached at: {} ({} + tokenizer.json)",
        gguf_dir.display(),
        LLM_MODEL_FILE,
    );
    Ok(gguf_dir.to_string_lossy().to_string())
}

/// Initialize and load the ReasoningEngine on CPU from a cached directory.
pub fn load_cached_model() -> std::result::Result<ReasoningEngine, Box<dyn std::error::Error + Send + Sync>> {
    let model_path = cached_model_path()
        .ok_or_else(|| "Llama-3.2-3B model not cached. Run `cotrader download-llm` first.".to_string())?;
    ReasoningEngine::load(&model_path)
}

/// Get the path to the cached model directory (where GGUF and tokenizer live together).
pub fn cached_model_path() -> Option<std::path::PathBuf> {
    let api = hf_hub::api::sync::Api::new().ok()?;
    let repo = api.model(LLM_MODEL_REPO.to_string());
    let gguf_path = repo.get(LLM_MODEL_FILE).ok()?;
    let dir = gguf_path.parent()?;
    // Verify tokenizer was also copied here
    if !dir.join("tokenizer.json").exists() {
        return None; // tokenizer not yet copied — download incomplete
    }
    Some(dir.to_path_buf())
}

/// Check if both the GGUF model and tokenizer are cached locally.
pub fn is_model_cached() -> bool {
    let api = match hf_hub::api::sync::Api::new() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let repo = api.model(LLM_MODEL_REPO.to_string());
    let gguf_path = match repo.get(LLM_MODEL_FILE) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let dir = match gguf_path.parent() {
        Some(d) => d,
        None => return false,
    };
    dir.join("tokenizer.json").exists()
}

/// Short public arbitration function for use from the tri-level validator.
///
/// If the engine is `None` or the gate decides no conflict, returns consensus.
/// Otherwise runs the LLM and returns the arbitrated signal.
/// Static helper — arbitrate with an optional engine.
pub fn arbitrate_with_engine(
    engine: Option<&mut ReasoningEngine>,
    input: &ArbitrationInput,
) -> Result<FinalSignal, Box<dyn std::error::Error + Send + Sync>> {
    match engine {
        Some(e) => e.arbitrate(input),
        None => {
            // No LLM loaded — compute consensus directly
            Ok(FinalSignal {
                llm_used: false,
                ..ReasoningEngine::compute_consensus(input)
            })
        }
    }
}

/// Named wrapper matching the user's requested API.
///
/// Reconciles Rules, ML, and Chronos signals into a final decision.
/// Only invokes the LLM when the escalation gate detects conflict or high risk.
pub fn arbitrate_complex_signals(
    engine: Option<&mut ReasoningEngine>,
    input: &ArbitrationInput,
) -> Result<FinalSignal, Box<dyn std::error::Error + Send + Sync>> {
    arbitrate_with_engine(engine, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _make_input(
        rules_action: &str, rules_conf: f64, rules_sig: f64,
        ml_action: &str, ml_conf: f64, ml_sig: f64,
        chronos_action: &str, chronos_conf: f64, chronos_sig: f64,
        regime: &str, vol_ratio: f64,
    ) -> ArbitrationInput {
        ArbitrationInput {
            rules_action: rules_action.into(),
            rules_confidence: rules_conf,
            rules_signal: rules_sig,
            ml_action: ml_action.into(),
            ml_confidence: ml_conf,
            ml_signal: ml_sig,
            chronos_action: chronos_action.into(),
            chronos_confidence: chronos_conf,
            chronos_signal: chronos_sig,
            market_regime: regime.into(),
            volatility_ratio: vol_ratio,
            symbol: "TEST".into(),
            current_price: 100.0,
            pattern_detected: "None".into(),
            pattern_confidence: 0.0,
            sentiment_score: 0.0,
            sentiment_confidence: 0.0,
            vector_memory_context: "Test vector memory".into(),
            news_context: "Test news context".into(),
            multi_tf_context: "Test multi-TF context".into(),
            agent_summary: "Test agent summary".into(),
        }
    }

    #[test]
    fn test_escalation_no_conflict() {
        let input = _make_input(
            "BUY", 0.7, 0.5,
            "BUY", 0.6, 0.4,
            "HOLD", 0.45, 0.1,
            "Ranging", 1.0,
        );
        assert!(!ReasoningEngine::escalation_triggered(&input));
    }

    #[test]
    fn test_escalation_direction_conflict() {
        let input = ArbitrationInput {
            rules_action: "BUY".into(),
            rules_confidence: 0.7,
            rules_signal: 0.5,
            ml_action: "BUY".into(),
            ml_confidence: 0.6,
            ml_signal: 0.4,
            chronos_action: "SELL".into(),
            chronos_confidence: 0.65,
            chronos_signal: -0.5,
            market_regime: "Ranging".into(),
            volatility_ratio: 1.0,
            symbol: "BTC".into(),
            current_price: 50000.0,
            pattern_detected: "None".into(),
            pattern_confidence: 0.0,
            sentiment_score: 0.0,
            sentiment_confidence: 0.0,
            vector_memory_context: "".into(),
            news_context: "".into(),
            multi_tf_context: "".into(),
            agent_summary: "".into(),
        };
        assert!(ReasoningEngine::escalation_triggered(&input));
    }

    #[test]
    fn test_escalation_high_volatility() {
        let input = ArbitrationInput {
            rules_action: "BUY".into(),
            rules_confidence: 0.7,
            rules_signal: 0.5,
            ml_action: "BUY".into(),
            ml_confidence: 0.6,
            ml_signal: 0.4,
            chronos_action: "HOLD".into(),
            chronos_confidence: 0.45,
            chronos_signal: 0.1,
            market_regime: "Ranging".into(),
            volatility_ratio: 2.0, // > 1.5 threshold
            symbol: "BTC".into(),
            current_price: 50000.0,
            pattern_detected: "None".into(),
            pattern_confidence: 0.0,
            sentiment_score: 0.0,
            sentiment_confidence: 0.0,
            vector_memory_context: "".into(),
            news_context: "".into(),
            multi_tf_context: "".into(),
            agent_summary: "".into(),
        };
        assert!(ReasoningEngine::escalation_triggered(&input));
    }

    #[test]
    fn test_escalation_volatile_regime() {
        let input = ArbitrationInput {
            rules_action: "BUY".into(),
            rules_confidence: 0.7,
            rules_signal: 0.5,
            ml_action: "BUY".into(),
            ml_confidence: 0.6,
            ml_signal: 0.4,
            chronos_action: "HOLD".into(),
            chronos_confidence: 0.45,
            chronos_signal: 0.1,
            market_regime: "Volatile".into(),
            volatility_ratio: 1.0,
            symbol: "BTC".into(),
            current_price: 50000.0,
            pattern_detected: "None".into(),
            pattern_confidence: 0.0,
            sentiment_score: 0.0,
            sentiment_confidence: 0.0,
            vector_memory_context: "".into(),
            news_context: "".into(),
            multi_tf_context: "".into(),
            agent_summary: "".into(),
        };
        assert!(ReasoningEngine::escalation_triggered(&input));
    }

    #[test]
    fn test_compute_consensus_buy() {
        let input = ArbitrationInput {
            rules_action: "BUY".into(),
            rules_confidence: 0.7,
            rules_signal: 0.5,
            ml_action: "BUY".into(),
            ml_confidence: 0.6,
            ml_signal: 0.4,
            chronos_action: "BUY".into(),
            chronos_confidence: 0.5,
            chronos_signal: 0.3,
            market_regime: "Ranging".into(),
            volatility_ratio: 1.0,
            symbol: "ETH".into(),
            current_price: 3000.0,
            pattern_detected: "None".into(),
            pattern_confidence: 0.0,
            sentiment_score: 0.0,
            sentiment_confidence: 0.0,
            vector_memory_context: "".into(),
            news_context: "".into(),
            multi_tf_context: "".into(),
            agent_summary: "".into(),
        };
        let signal = ReasoningEngine::compute_consensus(&input);
        assert_eq!(signal.direction, "BUY");
        assert!(!signal.llm_used);
        assert!(signal.confidence > 0.0);
    }

    #[test]
    fn test_compute_consensus_hold() {
        let input = ArbitrationInput {
            rules_action: "HOLD".into(),
            rules_confidence: 0.4,
            rules_signal: 0.1,
            ml_action: "HOLD".into(),
            ml_confidence: 0.5,
            ml_signal: 0.0,
            chronos_action: "HOLD".into(),
            chronos_confidence: 0.3,
            chronos_signal: 0.05,
            market_regime: "Ranging".into(),
            volatility_ratio: 0.8,
            symbol: "SOL".into(),
            current_price: 100.0,
            pattern_detected: "None".into(),
            pattern_confidence: 0.0,
            sentiment_score: 0.0,
            sentiment_confidence: 0.0,
            vector_memory_context: "".into(),
            news_context: "".into(),
            multi_tf_context: "".into(),
            agent_summary: "".into(),
        };
        let signal = ReasoningEngine::compute_consensus(&input);
        assert_eq!(signal.direction, "HOLD");
    }

    #[test]
    fn test_parse_response_buy() {
        let default = FinalSignal {
            direction: "HOLD".into(),
            confidence: 0.5,
            reasoning: "default".into(),
            llm_used: false,
        };
        let response = "DECISION: BUY\nCONFIDENCE: 0.75\nREASONING: Strong alignment across all layers with bullish momentum.";
        let result = ReasoningEngine::parse_response(response, &default);
        assert_eq!(result.direction, "BUY");
        assert!((result.confidence - 0.75).abs() < 0.01);
        assert!(result.llm_used);
    }

    #[test]
    fn test_parse_response_sell() {
        let default = FinalSignal {
            direction: "HOLD".into(),
            confidence: 0.5,
            reasoning: "default".into(),
            llm_used: false,
        };
        let response = "DECISION: SELL\nCONFIDENCE: 0.6\nREASONING: Bearish divergence with decreasing volume.";
        let result = ReasoningEngine::parse_response(response, &default);
        assert_eq!(result.direction, "SELL");
        assert!((result.confidence - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_action_to_num() {
        assert_eq!(ReasoningEngine::action_to_num("BUY"), 1);
        assert_eq!(ReasoningEngine::action_to_num("SELL"), -1);
        assert_eq!(ReasoningEngine::action_to_num("HOLD"), 0);
        assert_eq!(ReasoningEngine::action_to_num("BLOCK"), 0);
    }
}
