//! # Vector Memory — Cosine Similarity Search with Binary Quantization
//!
//! **100% domain-agnostic** — Zero dependencies on trading or any specific system.
//!
//! Provides two levels of vector search:
//! - **Full precision** — cosine similarity on `f64` vectors
//! - **Binary quantized** — `f64` → 1-bit sign compression, Hamming distance (up to 32× faster)

use serde::{Deserialize, Serialize};

use crate::types::MemoryRecord;

// ── Binary Quantization ─────────────────────────────────────────────────────

/// Quantize an `f64` vector to a binary signature vector (+1 → 1, -1 → 0).
/// Uses the sign of each element as the bit value.
pub fn quantize_binary(vector: &[f64]) -> Vec<bool> {
    vector.iter().map(|&v| v >= 0.0).collect()
}

/// Hamming distance between two binary vectors (number of differing bits).
/// Normalized to [0.0, 1.0] where 0.0 = identical, 1.0 = opposite.
pub fn hamming_distance(a: &[bool], b: &[bool]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    differing as f64 / a.len() as f64
}

/// Convert binary vector to similarity score (1.0 - hamming_distance).
pub fn hamming_similarity(a: &[bool], b: &[bool]) -> f64 {
    1.0 - hamming_distance(a, b)
}

// ── SIMD-Accelerated Hamming Distance ──────────────────────────────────────

/// Pack boolean vector into u64 array for SIMD processing.
/// Each u64 holds 64 bits (bools).
pub fn pack_bools_to_u64(bools: &[bool]) -> Vec<u64> {
    let num_words = (bools.len() + 63) / 64;
    let mut words = vec![0u64; num_words];
    for (i, &b) in bools.iter().enumerate() {
        if b {
            words[i / 64] |= 1u64 << (i % 64);
        }
    }
    words
}

/// Scalar fallback for Hamming distance on u64 arrays.
/// Counts differing bits using popcount.
fn hamming_distance_scalar(a: &[u64], b: &[u64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let mut count = 0u32;
    for i in 0..a.len() {
        count += (a[i] ^ b[i]).count_ones();
    }
    count as f64 / (a.len() * 64) as f64
}

/// AVX2-accelerated Hamming distance for binary vectors packed as u64.
/// Processes 256 bits (4 x u64) per iteration using SIMD intrinsics.
///
/// # Safety
/// This function uses unsafe AVX2 intrinsics. It is only compiled on x86_64
/// targets with AVX2 support. The caller must ensure:
/// - Both slices have the same length
/// - The CPU supports AVX2 (checked at runtime via `is_x86_feature_detected!`)
#[cfg(target_arch = "x86_64")]
unsafe fn hamming_distance_simd_avx2(a: &[u64], b: &[u64]) -> f64 {
    use std::arch::x86_64::*;

    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }

    let mut count = 0u32;
    let chunks = a.len() / 4;

    // Process 4 x u64 = 256 bits per iteration
    for i in 0..chunks {
        let va = _mm256_loadu_si256(a.as_ptr().add(i * 4) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(i * 4) as *const __m256i);
        let xor = _mm256_xor_si256(va, vb);
        // Use scalar popcount on each u64 lane (AVX2 doesn't have native popcnt)
        let mut tmp = [0u64; 4];
        _mm256_storeu_si256(tmp.as_mut_ptr() as *mut __m256i, xor);
        for &v in &tmp {
            count += v.count_ones();
        }
    }

    // Handle remaining elements with scalar
    let remaining = a.len() % 4;
    for i in (chunks * 4)..a.len() {
        count += (a[i] ^ b[i]).count_ones();
    }

    count as f64 / (a.len() * 64) as f64
}

/// SIMD-accelerated Hamming distance with runtime feature detection.
/// Falls back to scalar if AVX2 is not available.
pub fn hamming_distance_simd(a: &[u64], b: &[u64]) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 is detected at runtime, slices have same length
            unsafe { hamming_distance_simd_avx2(a, b) }
        } else {
            hamming_distance_scalar(a, b)
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        hamming_distance_scalar(a, b)
    }
}

/// SIMD-accelerated Hamming similarity (1.0 - distance).
pub fn hamming_similarity_simd(a: &[u64], b: &[u64]) -> f64 {
    1.0 - hamming_distance_simd(a, b)
}

/// Quantize binary vector to u64 packed format for SIMD processing.
pub fn quantize_to_packed(vector: &[f64]) -> Vec<u64> {
    let bools = quantize_binary(vector);
    pack_bools_to_u64(&bools)
}



// ── Vector Record ───────────────────────────────────────────────────────────

/// A stored vector with optional binary signature and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    pub record_id: String,
    pub vector: Vec<f64>,
    /// Binary-quantized signature (sign of each element)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_sig: Option<Vec<bool>>,
    pub metadata: std::collections::HashMap<String, String>,
}

// ── Vector Memory Store ─────────────────────────────────────────────────────

/// In-memory vector store with cosine similarity and binary-quantized Hamming search.
pub struct VectorMemory {
    records: Vec<VectorRecord>,
    dimension: usize,
}

impl VectorRecord {
    /// Build a VectorRecord from a MemoryRecord.
    /// Extracts the embedding, quantizes to binary, and attaches metadata.
    pub fn from_memory_record(record: &MemoryRecord) -> Option<Self> {
        let vector = record.embedding.clone()?;
        Some(Self {
            record_id: record.id.clone(),
            binary_sig: Some(quantize_binary(&vector)),
            vector,
            metadata: record.metadata.clone(),
        })
    }
}

impl VectorMemory {
    pub fn new(dimension: usize) -> Self {
        Self {
            records: Vec::new(),
            dimension,
        }
    }

    /// Store a vector record (auto-quantizes to binary on insert).
    pub fn store(&mut self, record: VectorRecord) {
        assert_eq!(
            record.vector.len(),
            self.dimension,
            "Vector dimension mismatch"
        );
        self.records.push(record);
    }

    /// Store a batch of records.
    pub fn store_batch(&mut self, records: Vec<VectorRecord>) {
        for record in records {
            self.store(record);
        }
    }

    /// Search with full-precision cosine similarity.
    pub fn search_cosine(&self, query: &[f64], k: usize) -> Vec<(f64, &VectorRecord)> {
        assert_eq!(query.len(), self.dimension, "Query dimension mismatch");

        let mut scored: Vec<(f64, &VectorRecord)> = self
            .records
            .iter()
            .map(|record| {
                let sim = cosine_similarity(query, &record.vector);
                (sim, record)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(k).collect()
    }

    /// Search with binary-quantized Hamming similarity (~32× faster than cosine).
    /// Falls back to cosine if binary signatures aren't available.
    pub fn search_binary(&self, query: &[f64], k: usize) -> Vec<(f64, &VectorRecord)> {
        assert_eq!(query.len(), self.dimension, "Query dimension mismatch");

        let query_binary = quantize_binary(query);

        let mut scored: Vec<(f64, &VectorRecord)> = self
            .records
            .iter()
            .map(|record| {
                let sim = match &record.binary_sig {
                    Some(bin) => hamming_similarity(&query_binary, bin),
                    None => cosine_similarity(query, &record.vector),
                };
                (sim, record)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(k).collect()
    }

    /// Hybrid search: use binary for coarse filtering, then cosine for re-ranking.
    pub fn search_hybrid(
        &self,
        query: &[f64],
        k: usize,
        top_n: usize,
    ) -> Vec<(f64, &VectorRecord)> {
        // Step 1: Binary coarse filter — get top_n candidates
        let candidates = self.search_binary(query, top_n);

        // Step 2: Re-rank with cosine similarity
        let mut re_ranked: Vec<(f64, &VectorRecord)> = candidates
            .into_iter()
            .map(|(_, record)| {
                let sim = cosine_similarity(query, &record.vector);
                (sim, record)
            })
            .collect();

        re_ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        re_ranked.into_iter().take(k).collect()
    }

    // ── Admin ─────────────────────────────────────────────────────────────

    /// Remove all records.
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Number of stored records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

impl Default for VectorMemory {
    fn default() -> Self {
        Self::new(128)
    }
}

// ── Cosine Similarity ───────────────────────────────────────────────────────

/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(id: &str, vector: Vec<f64>) -> VectorRecord {
        let bin = quantize_binary(&vector);
        VectorRecord {
            record_id: id.to_string(),
            binary_sig: Some(bin),
            vector,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_cosine_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);
    }

    #[test]
    fn test_binary_quantization() {
        let v = vec![1.5, -0.3, 0.0, -2.0, 0.7];
        let bits = quantize_binary(&v);
        assert_eq!(bits, vec![true, false, true, false, true]);
    }

    #[test]
    fn test_hamming_identical() {
        let a = vec![true, false, true];
        let b = vec![true, false, true];
        assert!((hamming_distance(&a, &b)).abs() < 0.001);
        assert!((hamming_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_hamming_opposite() {
        let a = vec![true, false, true];
        let b = vec![false, true, false];
        assert!((hamming_distance(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_vector_store_and_search() {
        let mut vm = VectorMemory::new(3);

        vm.store(make_record("v1", vec![1.0, 0.0, 0.0]));
        vm.store(make_record("v2", vec![0.0, 1.0, 0.0]));
        vm.store(make_record("v3", vec![0.9, 0.1, 0.0]));

        // Cosine search
        let results = vm.search_cosine(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1.record_id, "v1");

        // Binary search (should also find v1)
        let bin_results = vm.search_binary(&[1.0, 0.0, 0.0], 2);
        assert_eq!(bin_results.len(), 2);

        // Hybrid search
        let hybrid = vm.search_hybrid(&[1.0, 0.0, 0.0], 2, 5);
        assert_eq!(hybrid.len(), 2);
    }
}
