//! Local embedding engine using fastembed (AllMiniLM-L6-v2).
//!
//! Provides deterministic text→vector embeddings without external API calls.
//! Model is loaded once on first use and cached globally.

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::{Mutex, OnceLock};

static EMBEDDER: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();

fn get_embedder() -> &'static Mutex<TextEmbedding> {
    EMBEDDER.get_or_init(|| {
        let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
            .expect("Failed to load embedding model (all-MiniLM-L6-v2)");
        Mutex::new(model)
    })
}

/// Generate a 384-dimensional embedding for the given text.
pub fn embed_text(text: &str) -> Vec<f32> {
    let lock = get_embedder();
    let mut embedder = lock.lock().unwrap_or_else(|e| e.into_inner());
    match embedder.embed(vec![text], None) {
        Ok(embeddings) => embeddings
            .into_iter()
            .next()
            .unwrap_or_else(|| vec![0.0; 384]),
        Err(e) => {
            eprintln!("[Embeddings] Warning: embedding failed: {}", e);
            vec![0.0; 384]
        }
    }
}

/// Generate embeddings for multiple texts in batch.
pub fn embed_batch(texts: &[&str]) -> Vec<Vec<f32>> {
    let lock = get_embedder();
    let mut embedder = lock.lock().unwrap_or_else(|e| e.into_inner());
    match embedder.embed(texts.to_vec(), None) {
        Ok(embeddings) => embeddings,
        Err(e) => {
            eprintln!("[Embeddings] Warning: batch embedding failed: {}", e);
            texts.iter().map(|_| vec![0.0; 384]).collect()
        }
    }
}
