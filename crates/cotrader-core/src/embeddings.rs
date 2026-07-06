//! Ollama embedding engine using nomic-embed-text (768 dimensions).
//!
//! Generates 768-dimensional vector embeddings via Ollama's /api/embed endpoint.
//! This matches the agentic-memory server's expected dimension (MEMORY_VECTOR_DIM=768).
//!
//! Falls back to zero vectors if Ollama is unreachable.

use std::env;

/// Ollama embedding endpoint URL.
fn ollama_url() -> String {
    env::var("OLLAMA_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434".into())
}

/// Model name for embeddings.
fn embedding_model() -> String {
    env::var("OLLAMA_MODEL")
        .unwrap_or_else(|_| "nomic-embed-text".into())
}

/// Generate a 768-dimensional embedding for the given text (blocking).
///
/// Calls Ollama's /api/embed endpoint. Falls back to zero vector on error.
pub fn embed_text(text: &str) -> Vec<f32> {
    let url = format!("{}/api/embed", ollama_url());
    let body = serde_json::json!({
        "model": embedding_model(),
        "input": text,
    });

    match ureq::post(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send_json(&body)
    {
        Ok(resp) => {
            if let Ok(data) = resp.into_json::<serde_json::Value>() {
                if let Some(embeddings) = data["embeddings"].as_array() {
                    if let Some(first) = embeddings.first() {
                        if let Some(arr) = first.as_array() {
                            return arr.iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect();
                        }
                    }
                }
            }
            eprintln!("[Embeddings] Warning: unexpected Ollama response format");
            vec![0.0; 768]
        }
        Err(e) => {
            eprintln!("[Embeddings] Warning: Ollama embedding failed: {} — using zero vector", e);
            vec![0.0; 768]
        }
    }
}

/// Generate embeddings for multiple texts in batch (blocking).
///
/// Calls Ollama's /api/embed with multiple inputs.
pub fn embed_batch(texts: &[&str]) -> Vec<Vec<f32>> {
    let url = format!("{}/api/embed", ollama_url());
    let body = serde_json::json!({
        "model": embedding_model(),
        "input": texts,
    });

    match ureq::post(&url)
        .timeout(std::time::Duration::from_secs(30))
        .send_json(&body)
    {
        Ok(resp) => {
            if let Ok(data) = resp.into_json::<serde_json::Value>() {
                if let Some(embeddings) = data["embeddings"].as_array() {
                    return embeddings.iter()
                        .map(|emb| {
                            if let Some(arr) = emb.as_array() {
                                arr.iter()
                                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                                    .collect()
                            } else {
                                vec![0.0; 768]
                            }
                        })
                        .collect();
                }
            }
            eprintln!("[Embeddings] Warning: unexpected Ollama batch response");
            texts.iter().map(|_| vec![0.0; 768]).collect()
        }
        Err(e) => {
            eprintln!("[Embeddings] Warning: Ollama batch embedding failed: {}", e);
            texts.iter().map(|_| vec![0.0; 768]).collect()
        }
    }
}

/// Generate a 768-dimensional embedding for the given text (async).
///
/// Calls Ollama synchronously via spawn_blocking to avoid blocking Tokio workers.
pub async fn embed_text_async(text: &str) -> Vec<f32> {
    let text = text.to_string();
    tokio::task::spawn_blocking(move || embed_text(&text))
        .await
        .unwrap_or_else(|e| {
            eprintln!("[Embeddings] spawn_blocking failed: {}", e);
            vec![0.0; 768]
        })
}

/// Generate embeddings for multiple texts in batch (async).
///
/// Calls Ollama synchronously via spawn_blocking.
pub async fn embed_batch_async(texts: Vec<String>) -> Vec<Vec<f32>> {
    let count = texts.len();
    tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        embed_batch(&refs)
    })
    .await
    .unwrap_or_else(|e| {
        eprintln!("[Embeddings] spawn_blocking failed: {}", e);
        vec![vec![0.0; 768]; count]
    })
}
