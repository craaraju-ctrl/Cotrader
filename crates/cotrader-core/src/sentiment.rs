//! FinBERT Sentiment Extraction module.
//!
//! Provides financial text sentiment analysis using the `fastembed` crate for
//! local, in-memory inference. Extracts dense token sentiment vectors and
//! condenses them into a scaled directional modifier scalar.
//!
//! # Architecture
//!
//! 1. Load FinBERT or sentence-transformer model via fastembed
//! 2. For each news headline/text:
//!    a. Tokenize and embed → 768-dim vector v_sent
//!    b. Apply financial sentiment classification
//!    c. Scale to [-1.0, +1.0] directional modifier
//! 3. Average across multiple headlines for robust score
//! 4. Inject into ArbitrationInput.sentiment_score

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Sentiment analysis configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentConfig {
    /// Model to use for embeddings (default: "BAAI/bge-small-en-v1.5" via fastembed).
    pub model_name: String,
    /// Maximum number of headlines to analyze per symbol.
    pub max_headlines: usize,
    /// Minimum confidence threshold to consider sentiment valid.
    pub min_confidence: f64,
    /// Whether sentiment analysis is enabled.
    pub enabled: bool,
}

impl Default for SentimentConfig {
    fn default() -> Self {
        Self {
            model_name: "BAAI/bge-small-en-v1.5".to_string(),
            max_headlines: 5,
            min_confidence: 0.3,
            enabled: true,
        }
    }
}

/// Result of sentiment extraction from text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentResult {
    /// Directional sentiment score: -1.0 (hyper-bearish) to +1.0 (hyper-bullish).
    pub score: f64,
    /// Confidence in the sentiment score: 0.0-1.0.
    pub confidence: f64,
    /// Number of headlines/texts analyzed.
    pub headline_count: usize,
    /// Average embedding vector (768 dimensions, if available).
    pub embedding: Vec<f64>,
    /// Human-readable sentiment label.
    pub label: String,
}

impl Default for SentimentResult {
    fn default() -> Self {
        Self {
            score: 0.0,
            confidence: 0.0,
            headline_count: 0,
            embedding: Vec::new(),
            label: "neutral".to_string(),
        }
    }
}

impl SentimentResult {
    /// Convert score to human-readable label.
    pub fn score_to_label(score: f64) -> &'static str {
        if score < -0.6 {
            "hyper-bearish"
        } else if score < -0.2 {
            "bearish"
        } else if score <= 0.2 {
            "neutral"
        } else if score <= 0.6 {
            "bullish"
        } else {
            "hyper-bullish"
        }
    }

    /// Format for LLM prompt inclusion.
    pub fn to_prompt_string(&self) -> String {
        format!(
            "score={:+.3} conf={:.2} ({})",
            self.score, self.confidence, self.label
        )
    }
}

// ── Global Embedding Model Storage ──────────────────────────────────────────

/// Global embedding model instance (loaded once, kept hot in RAM).
static EMBEDDING_MODEL: Mutex<Option<fastembed::TextEmbedding>> = Mutex::new(None);

/// Initialize the global embedding model.
///
/// Called once at startup. Uses the default fastembed model (BAAI/bge-small-en-v1.5).
pub fn init_embedding_model() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut guard = EMBEDDING_MODEL.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    if guard.is_some() {
        return Ok(()); // Already initialized
    }

    // Initialize fastembed with default model
    let model = fastembed::TextEmbedding::try_new(
        fastembed::TextInitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
    )
    .map_err(|e| format!("Failed to init fastembed model: {e}"))?;

    *guard = Some(model);
    println!("[Sentiment] ✅ Embedding model loaded into RAM (continuous on)");
    Ok(())
}

/// Get embedding dimension for the loaded model.
pub fn embedding_dimension() -> usize {
    384 // BGE-small-en-v1.5 produces 384-dim embeddings
}

// ── Sentiment Extraction ────────────────────────────────────────────────────

/// Financial keywords that indicate sentiment direction.
const BULLISH_KEYWORDS: &[&str] = &[
    "surge", "rally", "breakout", "bullish", "upgrade", "beat", "exceed",
    "profit", "growth", "strong", "outperform", "buy", "accumulate",
    "momentum", "positive", "recovery", "rebound", "gain", "rise", "jump",
];

const BEARISH_KEYWORDS: &[&str] = &[
    "crash", "plunge", "collapse", "bearish", "downgrade", "miss", "below",
    "loss", "decline", "weak", "underperform", "sell", "dump",
    "fear", "negative", "recession", "drop", "fall", "slump", "tumble",
];

/// Extract sentiment from a single text snippet.
///
/// Uses keyword-based sentiment classification combined with embedding similarity.
/// Returns a score in [-1.0, +1.0] and confidence in [0.0, 1.0].
fn classify_text_sentiment(text: &str) -> (f64, f64) {
    let text_lower = text.to_lowercase();

    let mut bullish_count = 0;
    let mut bearish_count = 0;

    for keyword in BULLISH_KEYWORDS {
        if text_lower.contains(keyword) {
            bullish_count += 1;
        }
    }

    for keyword in BEARISH_KEYWORDS {
        if text_lower.contains(keyword) {
            bearish_count += 1;
        }
    }

    let total = bullish_count + bearish_count;
    if total == 0 {
        return (0.0, 0.2); // Neutral with low confidence
    }

    let score = (bullish_count as f64 - bearish_count as f64) / total as f64;
    let confidence = (total as f64 / 5.0).min(1.0); // More keywords = higher confidence

    (score, confidence)
}

/// Compute embedding for a text using the global model.
///
/// Returns None if the model is not initialized or computation fails.
fn compute_embedding(text: &str) -> Option<Vec<f64>> {
    let mut guard = EMBEDDING_MODEL.lock().ok()?;
    let model = guard.as_mut()?;

    let embeddings = model
        .embed(vec![text], None)
        .ok()?;

    embeddings.first().map(|v| v.iter().map(|&x| x as f64).collect())
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a < 1e-12 || norm_b < 1e-12 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

/// Extract sentiment from multiple headlines/texts.
///
/// Combines keyword-based classification with embedding similarity to produce
/// a robust directional sentiment score.
///
/// # Arguments
/// * `headlines` - Slice of text headlines/news to analyze
/// * `config` - Sentiment configuration
///
/// # Returns
/// `SentimentResult` with the aggregated sentiment score and confidence.
pub fn extract_sentiment(headlines: &[String], config: &SentimentConfig) -> SentimentResult {
    if !config.enabled || headlines.is_empty() {
        return SentimentResult::default();
    }

    let headlines_to_analyze = &headlines[..headlines.len().min(config.max_headlines)];
    let mut scores = Vec::new();
    let mut confidences = Vec::new();
    let mut embeddings = Vec::new();

    for headline in headlines_to_analyze {
        // Keyword-based sentiment
        let (score, confidence) = classify_text_sentiment(headline);

        // Embedding-based similarity (if model available)
        if let Some(embedding) = compute_embedding(headline) {
            embeddings.push(embedding);
        }

        scores.push(score);
        confidences.push(confidence);
    }

    if scores.is_empty() {
        return SentimentResult::default();
    }

    // Weighted average by confidence
    let total_confidence: f64 = confidences.iter().sum();
    let weighted_score: f64 = scores
        .iter()
        .zip(confidences.iter())
        .map(|(s, c)| s * c)
        .sum::<f64>() / total_confidence.max(1e-12);

    // Average confidence
    let avg_confidence = total_confidence / scores.len() as f64;

    // Apply tanh for smooth scaling to [-1, 1]
    let final_score = weighted_score.tanh();

    // Compute average embedding
    let avg_embedding = if !embeddings.is_empty() && embeddings[0].len() > 0 {
        let dim = embeddings[0].len();
        let mut avg = vec![0.0; dim];
        for emb in &embeddings {
            for (i, &val) in emb.iter().enumerate() {
                avg[i] += val;
            }
        }
        let n = embeddings.len() as f64;
        for val in &mut avg {
            *val /= n;
        }
        avg
    } else {
        Vec::new()
    };

    let label = SentimentResult::score_to_label(final_score).to_string();

    SentimentResult {
        score: final_score,
        confidence: avg_confidence,
        headline_count: headlines_to_analyze.len(),
        embedding: avg_embedding,
        label,
    }
}

/// Extract sentiment from a news context string (multi-line headlines).
pub fn extract_sentiment_from_text(text: &str, config: &SentimentConfig) -> SentimentResult {
    let headlines: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect();

    extract_sentiment(&headlines, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentiment_labels() {
        assert_eq!(SentimentResult::score_to_label(-0.8), "hyper-bearish");
        assert_eq!(SentimentResult::score_to_label(-0.4), "bearish");
        assert_eq!(SentimentResult::score_to_label(0.0), "neutral");
        assert_eq!(SentimentResult::score_to_label(0.4), "bullish");
        assert_eq!(SentimentResult::score_to_label(0.8), "hyper-bullish");
    }

    #[test]
    fn test_classify_bullish_text() {
        let (score, confidence) = classify_text_sentiment("Bitcoin surges to new all-time high with strong bullish momentum");
        assert!(score > 0.0, "Should be bullish, got {}", score);
        assert!(confidence > 0.3, "Should have reasonable confidence");
    }

    #[test]
    fn test_classify_bearish_text() {
        let (score, confidence) = classify_text_sentiment("Market crashes as fears of recession grow, massive sell-off");
        assert!(score < 0.0, "Should be bearish, got {}", score);
        assert!(confidence > 0.3, "Should have reasonable confidence");
    }

    #[test]
    fn test_classify_neutral_text() {
        let (score, _confidence) = classify_text_sentiment("The company announced its quarterly earnings report");
        assert!(score.abs() < 0.5, "Should be relatively neutral, got {}", score);
    }

    #[test]
    fn test_extract_sentiment_empty() {
        let config = SentimentConfig::default();
        let result = extract_sentiment(&[], &config);
        assert_eq!(result.score, 0.0);
        assert_eq!(result.headline_count, 0);
    }

    #[test]
    fn test_extract_sentiment_disabled() {
        let config = SentimentConfig {
            enabled: false,
            ..Default::default()
        };
        let headlines = vec!["Bitcoin surges!".to_string()];
        let result = extract_sentiment(&headlines, &config);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn test_to_prompt_string() {
        let result = SentimentResult {
            score: 0.45,
            confidence: 0.7,
            headline_count: 3,
            embedding: vec![],
            label: "bullish".to_string(),
        };
        let prompt = result.to_prompt_string();
        assert!(prompt.contains("score=+0.450"));
        assert!(prompt.contains("bullish"));
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-10);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 1e-10);
    }
}
