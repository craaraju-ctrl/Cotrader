//! # Memory HTTP API — REST Interface for Agentic Memory
//!
//! Extended with tier-aware, graph, consolidation, evolution, reasoning, and expert endpoints.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode as AxumStatus,
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;

use crate::consolidation::ConsolidationEngine;
use crate::context::ContextManager;
use crate::errors::AppError;
use crate::evolution::{EvolutionConfig, EvolutionEngine};
use crate::graph::KnowledgeGraph;
use crate::rag::Embedder;
use crate::reasoning::ReasoningEngine;
use crate::reflection::ReflectionEngine;
use crate::store::MemoryStore;
use crate::temporal::TemporalEngine;
use crate::tiers::TieredMemory;
use crate::types::{
    ContextBlock, MemoryRecord, MemoryTier, SearchResult, StorageConfig,
};

// ── API State ──────────────────────────────────────────────────────────────

/// Shared state that can be cloned for axum.
/// Uses `tokio::sync::RwLock` for read-heavy workloads (searches, gets)
/// to allow concurrent reads while serializing writes.
#[derive(Clone)]
pub struct ApiState {
    pub store: MemoryStore,
    pub embedder: Option<Arc<dyn Embedder>>,
    pub tiered: Arc<RwLock<TieredMemory>>,
    pub graph: Arc<KnowledgeGraph>,
    pub reasoning: Arc<ReasoningEngine>,
    pub consolidation: Arc<ConsolidationEngine>,
    pub evolution: Arc<RwLock<EvolutionEngine>>,
    pub reflection: Arc<ReflectionEngine>,
    pub temporal: Arc<TemporalEngine>,
    pub context: Arc<RwLock<ContextManager>>,
}

// ── Request Logging Middleware ─────────────────────────────────────────────

/// Logs each incoming request with method, URI, and response status + latency.
async fn logging_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = std::time::Instant::now();
    let resp = next.run(req).await;
    let status = resp.status();
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis();

    // Record Prometheus metrics
    crate::metrics::record_http_request(method.as_str(), uri.path(), status.as_u16(), elapsed_ms);

    tracing::info!("{} {} {} {:?}", method, uri, status, elapsed);
    resp
}

// ── Simple API Key Authentication Middleware ───────────────────────────────

/// Basic authentication middleware.
async fn auth_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<axum::response::Response, AppError> {
    let api_key = std::env::var("MEMORY_API_KEY").ok();

    if let Some(expected_key) = api_key {
        let headers = req.headers();

        let provided_key = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()));

        match provided_key {
            Some(key) if key == expected_key => {}
            _ => {
                return Err(AppError::BadRequest(
                    "Unauthorized: Invalid or missing API key".to_string(),
                ));
            }
        }
    }

    Ok(next.run(req).await)
}

// ── Request/Response Types ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(rename = "type")]
    content_type: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    limit: Option<usize>,
    tier: Option<String>,
}

#[derive(Deserialize)]
pub struct SemanticSearchBody {
    query_vec: Vec<f64>,
    k: Option<usize>,
}

#[derive(Deserialize)]
pub struct EmbedBody {
    text: String,
    model: Option<String>,
}

#[derive(Serialize)]
pub struct EmbedResponse {
    pub embedding: Vec<f64>,
    pub dimension: usize,
    pub model: String,
}

#[derive(Deserialize)]
pub struct InsertRecordBody {
    pub id: String,
    pub content: String,
    pub content_type: String,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub embedding: Option<Vec<f64>>,
    pub timestamp: Option<String>,
    pub tier: Option<String>,
    pub importance: Option<f64>,
    #[serde(default)]
    pub namespace_id: Option<String>,
}

#[derive(Deserialize)]
pub struct AddEdgeBody {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub weight: Option<f64>,
}

#[derive(Deserialize)]
pub struct BfsQuery {
    pub start_id: String,
    pub max_depth: Option<u32>,
    pub relation_type: Option<String>,
}

#[derive(Deserialize)]
pub struct ReasonBody {
    pub goal: String,
    pub context_query: String,
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct ConsolidateBody {
    pub content_type: Option<String>,
}

#[derive(Deserialize)]
pub struct ReflectionBody {
    pub topic: String,
    pub monologue: String,
    pub conclusion: String,
    #[serde(default)]
    pub planned_actions: Vec<String>,
    pub confidence: Option<f64>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
pub struct TemporalFactBody {
    pub content: String,
    pub content_type: String,
    pub importance: Option<f64>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct TemporalRecallBody {
    pub fact_id: String,
}

#[derive(Deserialize)]
pub struct ContextBlockBody {
    pub block_id: String,
    pub label: String,
    pub content: String,
    #[serde(default)]
    pub pinned: bool,
    pub priority: Option<i32>,
    pub max_tokens: Option<usize>,
}

#[derive(Deserialize)]
pub struct SearchTempQuery {
    pub q: Option<String>,
    pub content_type: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct CheckAccessBody {
    pub reader_namespace: String,
    pub target_namespace: String,
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Records CRUD
// ══════════════════════════════════════════════════════════════════════════

async fn insert_record(
    State(state): State<ApiState>,
    Json(body): Json<InsertRecordBody>,
) -> Result<impl IntoResponse, AppError> {
    let tier = body
        .tier
        .as_deref()
        .and_then(|t| t.parse::<MemoryTier>().ok())
        .unwrap_or(MemoryTier::Episodic);

    let importance = body.importance.unwrap_or(0.5);

    let timestamp = body
        .timestamp
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let record = MemoryRecord {
        id: body.id,
        content: body.content,
        content_type: body.content_type,
        metadata: body.metadata,
        embedding: body.embedding,
        timestamp,
    };

    let namespace_id = body.namespace_id.unwrap_or_else(|| "default".to_string());

    if tier == MemoryTier::Working {
        // Working memory now supports namespace isolation
        let mut tiered = state.tiered.write().await;
        tiered
            .insert_with_namespace(record.clone(), tier, importance, &namespace_id)
            .map_err(|e| AppError::Internal(format!("Insert failed: {}", e)))?;
    } else {
        // All other tiers go through namespace-aware insertion
        // Validate namespace exists to prevent orphaned records
        if namespace_id != "default" {
            let ns_exists = state
                .store
                .get_namespace(&namespace_id)
                .map_err(|e| AppError::Database(format!("Namespace lookup failed: {}", e)))?;
            if ns_exists.is_none() {
                return Err(AppError::NotFound(format!(
                    "Namespace '{}' does not exist. Create it first via POST /namespaces.",
                    namespace_id
                )));
            }
        }

        state
            .store
            .insert_into_tier_with_namespace(&record, tier, importance, None, None, &namespace_id)
            .map_err(|e| AppError::Internal(format!("Insert failed: {}", e)))?;
    }

    Ok((
        AxumStatus::CREATED,
        Json(serde_json::json!({"id": record.id, "tier": tier.to_string()})),
    ))
}

async fn get_record(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut tiered = state.tiered.write().await;
    match tiered.get(&id) {
        Ok(Some(t_record)) => Ok((AxumStatus::OK, Json(t_record))),
        Ok(None) => Err(AppError::NotFound(format!("Record '{}' not found", id))),
        Err(e) => Err(AppError::Database(format!("Failed to get record: {}", e))),
    }
}

async fn delete_record(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.store.delete(&id) {
        Ok(true) => Ok((AxumStatus::OK, Json(serde_json::json!({"deleted": true})))),
        Ok(false) => Err(AppError::NotFound(format!("Record '{}' not found", id))),
        Err(e) => Err(AppError::Database(format!("Failed to delete: {}", e))),
    }
}

async fn list_records(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(1000);
    let offset = query.offset.unwrap_or(0);

    let result = if let Some(content_type) = &query.content_type {
        state.store.list_by_type(content_type, limit, offset)
    } else {
        state.store.all(limit, offset)
    };

    match result {
        Ok(records) => Ok((AxumStatus::OK, Json(records))),
        Err(e) => Err(AppError::Database(format!("Failed to list records: {}", e))),
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Search
// ══════════════════════════════════════════════════════════════════════════

async fn search_records(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(100);

    let results = if let Some(tier_str) = &query.tier {
        if let Ok(tier) = tier_str.parse::<MemoryTier>() {
            state
                .store
                .search_fts_in_tier(&query.q, tier, limit)
                .map(|r| {
                    r.into_iter()
                        .map(|(record, rank)| SearchResult {
                            record,
                            score: (1.0 / (1.0 + rank.abs())).clamp(0.0, 1.0),
                            method: "fts".into(),
                        })
                        .collect::<Vec<SearchResult>>()
                })
        } else {
            Err(rusqlite::Error::InvalidParameterName(tier_str.clone()))
        }
    } else {
        state.store.search_fts(&query.q, limit).map(|r| {
            r.into_iter()
                .map(|(record, rank)| SearchResult {
                    record,
                    score: (1.0 / (1.0 + rank.abs())).clamp(0.0, 1.0),
                    method: "fts".into(),
                })
                .collect::<Vec<SearchResult>>()
        })
    };

    match results {
        Ok(results) => Ok((AxumStatus::OK, Json(results))),
        Err(e) => Err(AppError::Database(format!("Search failed: {}", e))),
    }
}

async fn search_semantic(
    State(state): State<ApiState>,
    Json(body): Json<SemanticSearchBody>,
) -> Result<impl IntoResponse, AppError> {
    let k = body.k.unwrap_or(10).min(100);

    match state.store.search_vectors(&body.query_vec, k) {
        Ok(results) => {
            let search_results: Vec<SearchResult> = results
                .into_iter()
                .map(|(record, distance)| {
                    let score = ((2.0 - distance as f64) / 2.0).clamp(0.0, 1.0);
                    SearchResult {
                        record,
                        score,
                        method: "sqlite-vec".into(),
                    }
                })
                .collect();
            Ok((AxumStatus::OK, Json(search_results)))
        }
        Err(_e) => match state.store.all_with_embeddings() {
            Ok(records) => {
                let mut scored: Vec<SearchResult> = records
                    .into_iter()
                    .filter_map(|record| {
                        let emb = record.embedding.as_ref()?;
                        let score =
                            (crate::vector::cosine_similarity(&body.query_vec, emb) + 1.0) / 2.0;
                        Some(SearchResult {
                            record,
                            score,
                            method: "semantic".into(),
                        })
                    })
                    .collect();
                scored.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                scored.truncate(k);
                Ok((AxumStatus::OK, Json(scored)))
            }
            Err(e2) => Err(AppError::Internal(format!(
                "Semantic search failed: {}",
                e2
            ))),
        },
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Embed / Semantic
// ══════════════════════════════════════════════════════════════════════════

async fn embed_text(
    State(state): State<ApiState>,
    Json(body): Json<EmbedBody>,
) -> Result<impl IntoResponse, AppError> {
    let embedder = match &state.embedder {
        Some(e) => e,
        None => {
            return Err(AppError::Internal(
                "No embedder configured. Set OLLAMA_BASE_URL and OLLAMA_MODEL env vars."
                    .to_string(),
            ));
        }
    };

    match embedder.embed(&body.text).await {
        Ok(embedding) => {
            let dim = embedding.len();
            let model = body.model.unwrap_or_else(|| "default".to_string());
            Ok((
                AxumStatus::OK,
                Json(EmbedResponse {
                    embedding,
                    dimension: dim,
                    model,
                }),
            ))
        }
        Err(e) => Err(AppError::Internal(format!("Embedding failed: {}", e))),
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Tier Operations
// ══════════════════════════════════════════════════════════════════════════

async fn list_by_tier(
    State(state): State<ApiState>,
    Path(tier_str): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let tier = tier_str
        .parse::<MemoryTier>()
        .map_err(AppError::BadRequest)?;

    let limit = query.limit.unwrap_or(50).min(1000);
    let offset = query.offset.unwrap_or(0);

    match state.store.list_by_tier(tier, limit, offset) {
        Ok(records) => Ok((AxumStatus::OK, Json(records))),
        Err(e) => Err(AppError::Database(format!("Failed to list tier: {}", e))),
    }
}

async fn promote_record(
    State(state): State<ApiState>,
    Path((id, target_tier)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let tier = target_tier
        .parse::<MemoryTier>()
        .map_err(AppError::BadRequest)?;

    match state.store.promote(&id, tier) {
        Ok(true) => Ok((
            AxumStatus::OK,
            Json(serde_json::json!({"promoted": id, "to": tier.to_string()})),
        )),
        Ok(false) => Err(AppError::NotFound(format!("Record '{}' not found", id))),
        Err(e) => Err(AppError::Database(format!("Failed to promote: {}", e))),
    }
}

async fn flush_working(State(state): State<ApiState>) -> Result<impl IntoResponse, AppError> {
    let mut tiered = state.tiered.write().await;
    match tiered.flush_all_working() {
        Ok(count) => Ok((AxumStatus::OK, Json(serde_json::json!({"flushed": count})))),
        Err(e) => Err(AppError::Database(format!(
            "Failed to flush working memory: {}",
            e
        ))),
    }
}

async fn run_auto_promotion(
    State(state): State<ApiState>,
    Path(tier_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let tier = tier_str
        .parse::<MemoryTier>()
        .map_err(AppError::BadRequest)?;
    let tiered = state.tiered.read().await;
    let count = tiered
        .run_auto_promotion(tier)
        .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({"auto_promoted": count, "tier": tier.to_string()})),
    ))
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Graph Operations
// ══════════════════════════════════════════════════════════════════════════

async fn add_graph_edge(
    State(state): State<ApiState>,
    Json(body): Json<AddEdgeBody>,
) -> Result<impl IntoResponse, AppError> {
    let weight = body.weight.unwrap_or(1.0);
    match state.graph.add_edge(
        &body.source_id,
        &body.target_id,
        &body.relation_type,
        weight,
    ) {
        Ok(edge_id) => Ok((
            AxumStatus::CREATED,
            Json(serde_json::json!({"edge_id": edge_id})),
        )),
        Err(e) => Err(AppError::Database(format!(
            "Failed to add graph edge: {}",
            e
        ))),
    }
}

async fn get_graph_edges(
    State(state): State<ApiState>,
    Path(record_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.graph.get_edges(&record_id) {
        Ok(edges) => Ok((AxumStatus::OK, Json(edges))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn bfs_graph(
    State(state): State<ApiState>,
    Json(body): Json<BfsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let max_depth = body.max_depth.unwrap_or(3);
    let results = state
        .graph
        .bfs(&body.start_id, max_depth, body.relation_type.as_deref())
        .map_err(|e| AppError::Internal(format!("BFS failed: {}", e)))?;
    Ok((AxumStatus::OK, Json(results)))
}

async fn get_related_records(
    State(state): State<ApiState>,
    Path(record_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.graph.get_related(&record_id, None, 3) {
        Ok(related) => Ok((AxumStatus::OK, Json(related))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn graph_hubs(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(20).min(100);
    match state.graph.get_hubs(limit) {
        Ok(hubs) => Ok((AxumStatus::OK, Json(hubs))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Reasoning
// ══════════════════════════════════════════════════════════════════════════

async fn reason_about(
    State(state): State<ApiState>,
    Json(body): Json<ReasonBody>,
) -> Result<impl IntoResponse, AppError> {
    let tags = body.tags.unwrap_or_default();
    let chain = state.reasoning.start_chain(&body.goal, tags);
    Ok((AxumStatus::OK, Json(chain)))
}

async fn get_reasoning_chain(
    State(state): State<ApiState>,
    Path(chain_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.reasoning.get_chain(&chain_id) {
        Ok(Some(chain)) => Ok((AxumStatus::OK, Json(chain))),
        Ok(None) => Err(AppError::NotFound(format!(
            "Chain '{}' not found",
            chain_id
        ))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn search_reasoning_chains(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(100);
    match state.reasoning.search_chains(&query.q, limit) {
        Ok(chains) => Ok((AxumStatus::OK, Json(chains))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn distill_reasoning_chain(
    State(state): State<ApiState>,
    Path(chain_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.reasoning.distill_to_semantic(&chain_id) {
        Ok(record_id) => Ok((
            AxumStatus::OK,
            Json(serde_json::json!({"distilled_to": record_id})),
        )),
        Err(e) => Err(AppError::BadRequest(e)),
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Consolidation
// ══════════════════════════════════════════════════════════════════════════

async fn run_consolidation(State(state): State<ApiState>) -> impl IntoResponse {
    let report = state.consolidation.run_cycle();
    (AxumStatus::OK, Json(report))
}

async fn analyze_tiers(State(state): State<ApiState>) -> Result<impl IntoResponse, AppError> {
    let mut analyses = Vec::new();
    for tier in MemoryTier::all() {
        let config = state
            .store
            .get_tier_config(tier)
            .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
        let records = state
            .store
            .list_by_tier(tier, 1000, 0)
            .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
        analyses.push(serde_json::json!({
            "tier": tier.to_string(),
            "config": config,
            "record_count": records.len(),
        }));
    }
    Ok((AxumStatus::OK, Json(analyses)))
}

async fn detect_conflicts(
    State(state): State<ApiState>,
    Json(body): Json<ConsolidateBody>,
) -> Result<impl IntoResponse, AppError> {
    let ct = body.content_type.unwrap_or_else(|| "fact".to_string());
    match state.consolidation.detect_conflicts(&ct) {
        Ok(conflicts) => Ok((AxumStatus::OK, Json(conflicts))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Evolution & Sleep Cycle
// ══════════════════════════════════════════════════════════════════════════

async fn run_evolution(State(state): State<ApiState>) -> Result<impl IntoResponse, AppError> {
    let evo = state.evolution.read().await;
    let events = evo.tune_tiers();
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({"tuning_events": events.len(), "events": events})),
    ))
}

/// Run a full sleep cycle: consolidation + tier tuning + stale pruning +
/// procedural distillation + reflexion. This is the main self-evolution
/// endpoint that exercises the entire chain-of-reasoning pipeline.
async fn run_sleep_cycle(State(state): State<ApiState>) -> Result<impl IntoResponse, AppError> {
    // Guard against re-entrant sleep cycles
    if state.evolution.read().await.is_sleeping() {
        return Err(AppError::Conflict(
            "Sleep cycle already in progress. Wait for the current cycle to complete.".to_string(),
        ));
    }
    let mut evo = state.evolution.write().await;
    let report = evo.run_sleep_cycle();
    Ok((AxumStatus::OK, Json(report)))
}

async fn get_evolution_events(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(200);
    let event_type = if query.q.is_empty() {
        None
    } else {
        Some(query.q.as_str())
    };
    match state.store.get_evolution_events(event_type, limit) {
        Ok(events) => Ok((AxumStatus::OK, Json(events))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Reflections
// ══════════════════════════════════════════════════════════════════════════

async fn create_reflection(
    State(state): State<ApiState>,
    Json(body): Json<ReflectionBody>,
) -> Result<impl IntoResponse, AppError> {
    let reflection = state.reflection.reflect(
        &body.topic,
        &body.monologue,
        &body.conclusion,
        body.planned_actions,
        body.confidence.unwrap_or(0.5),
        body.tags,
    );
    Ok((AxumStatus::CREATED, Json(reflection)))
}

async fn get_reflection(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.reflection.get_reflection(&id) {
        Some(r) => Ok((AxumStatus::OK, Json(r))),
        None => Err(AppError::NotFound(format!("Reflection '{}' not found", id))),
    }
}

async fn list_reflections(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(200);
    let reflections = state.reflection.list_reflections(limit);
    Ok((AxumStatus::OK, Json(reflections)))
}

async fn search_reflections(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(100);
    match state.store.search_reflections(&query.q, limit) {
        Ok(reflections) => Ok((AxumStatus::OK, Json(reflections))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn delete_reflection(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.reflection.delete_reflection(&id) {
        true => Ok((AxumStatus::OK, Json(serde_json::json!({"deleted": true})))),
        false => Err(AppError::NotFound(format!("Reflection '{}' not found", id))),
    }
}

async fn run_self_assessment(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, AppError> {
    let assessment = state.reflection.self_assess();
    Ok((AxumStatus::OK, Json(assessment)))
}

async fn get_assessment(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.reflection.get_assessment(&id) {
        Some(a) => Ok((AxumStatus::OK, Json(a))),
        None => Err(AppError::NotFound(format!("Assessment '{}' not found", id))),
    }
}

async fn list_assessments(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(50);
    let assessments = state.reflection.list_assessments(limit);
    Ok((AxumStatus::OK, Json(assessments)))
}

async fn run_reflexion(
    State(state): State<ApiState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let topic = body["topic"].as_str().unwrap_or("general");
    let observation = body["observation"].as_str().unwrap_or("");
    let (reflection, assessment) = state.reflection.reflexion_loop(topic, observation);
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({
            "reflection": reflection,
            "assessment": assessment,
        })),
    ))
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Namespaces
// ══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct CreateNamespaceBody {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub owner: String,
    #[serde(default)]
    pub read_parents: Vec<String>,
    #[serde(default)]
    pub write_children: Vec<String>,
}

#[derive(Deserialize)]
pub struct NamespaceQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

async fn create_namespace(
    State(state): State<ApiState>,
    Json(body): Json<CreateNamespaceBody>,
) -> Result<impl IntoResponse, AppError> {
    let ns_id = format!("ns_{}", crate::generate_id());
    state
        .store
        .create_namespace(
            &ns_id,
            &body.name,
            &body.description,
            &body.owner,
            &body.read_parents,
            &body.write_children,
        )
        .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
    Ok((
        AxumStatus::CREATED,
        Json(serde_json::json!({
            "namespace_id": ns_id,
            "name": body.name,
        })),
    ))
}

async fn list_namespaces(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, AppError> {
    let namespaces = state
        .store
        .list_namespaces()
        .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
    Ok((AxumStatus::OK, Json(namespaces)))
}

async fn get_namespace(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.store.get_namespace(&id) {
        Ok(Some(ns)) => Ok((AxumStatus::OK, Json(ns))),
        Ok(None) => Err(AppError::NotFound(format!("Namespace '{}' not found", id))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn delete_namespace(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.store.delete_namespace(&id) {
        Ok(true) => Ok((AxumStatus::OK, Json(serde_json::json!({"deleted": true})))),
        Ok(false) => Err(AppError::NotFound(format!("Namespace '{}' not found", id))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn list_records_in_namespace(
    State(state): State<ApiState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(1000);
    let offset = query.offset.unwrap_or(0);
    match state.store.list_by_namespace(&namespace_id, limit, offset) {
        Ok(records) => Ok((AxumStatus::OK, Json(records))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn search_in_namespace(
    State(state): State<ApiState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(100);
    match state.store.search_fts_in_namespace(&query.q, &namespace_id, limit) {
        Ok(results) => {
            let search_results: Vec<SearchResult> = results
                .into_iter()
                .map(|(record, rank)| SearchResult {
                    record,
                    score: (1.0 / (1.0 + rank.abs())).clamp(0.0, 1.0),
                    method: "fts_namespace".into(),
                })
                .collect();
            Ok((AxumStatus::OK, Json(search_results)))
        }
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn namespace_stats(
    State(state): State<ApiState>,
    Path(namespace_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let count = state
        .store
        .count_by_namespace(&namespace_id)
        .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({
            "namespace_id": namespace_id,
            "record_count": count,
        })),
    ))
}

async fn get_namespace_by_name(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.store.get_namespace_by_name(&name) {
        Ok(Some(ns)) => Ok((AxumStatus::OK, Json(ns))),
        Ok(None) => Err(AppError::NotFound(format!(
            "Namespace '{}' not found",
            name
        ))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn check_namespace_access(
    State(state): State<ApiState>,
    Json(body): Json<CheckAccessBody>,
) -> Result<impl IntoResponse, AppError> {
    let can_read = state
        .store
        .can_read_namespace(&body.reader_namespace, &body.target_namespace)
        .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({
            "reader": body.reader_namespace,
            "target": body.target_namespace,
            "can_read": can_read,
        })),
    ))
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Temporal Facts
// ══════════════════════════════════════════════════════════════════════════

async fn store_temporal_fact(
    State(state): State<ApiState>,
    Json(body): Json<TemporalFactBody>,
) -> Result<impl IntoResponse, AppError> {
    let fact = state.temporal.store_fact(
        &body.content,
        &body.content_type,
        body.importance.unwrap_or(0.5),
        body.metadata,
    );
    Ok((AxumStatus::CREATED, Json(fact)))
}

async fn get_temporal_fact(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.temporal.get_fact(&id) {
        Some(f) => Ok((AxumStatus::OK, Json(f))),
        None => Err(AppError::NotFound(format!("Temporal fact '{}' not found", id))),
    }
}

async fn list_temporal_facts(
    State(state): State<ApiState>,
    Query(query): Query<SearchTempQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(200);
    let ct = query.content_type.as_deref();
    let facts = state.temporal.list_current_facts(ct, limit);
    Ok((AxumStatus::OK, Json(facts)))
}

async fn search_temporal_facts(
    State(state): State<ApiState>,
    Query(query): Query<SearchTempQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(100);
    let q = query.q.unwrap_or_default();
    let facts = state.temporal.search_facts(&q, limit);
    Ok((AxumStatus::OK, Json(facts)))
}

async fn recall_temporal_fact(
    State(state): State<ApiState>,
    Json(body): Json<TemporalRecallBody>,
) -> Result<impl IntoResponse, AppError> {
    match state.temporal.get_fact(&body.fact_id) {
        Some(mut fact) => {
            state.temporal.recall_fact(&mut fact);
            Ok((AxumStatus::OK, Json(fact)))
        }
        None => Err(AppError::NotFound(format!(
            "Temporal fact '{}' not found",
            body.fact_id
        ))),
    }
}

async fn temporal_decay_score(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.temporal.get_fact(&id) {
        Some(fact) => {
            let decay = state.temporal.calculate_decay(&fact);
            let effective = state.temporal.effective_importance(&fact);
            let stale = state.temporal.is_stale(&fact);
            Ok((
                AxumStatus::OK,
                Json(serde_json::json!({
                    "fact_id": id,
                    "decay_score": decay,
                    "effective_importance": effective,
                    "is_stale": stale,
                    "recall_count": fact.recall_count,
                })),
            ))
        }
        None => Err(AppError::NotFound(format!(
            "Temporal fact '{}' not found",
            id
        ))),
    }
}

async fn get_temporal_version_chain(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let chain = state.temporal.get_version_chain(&id);
    Ok((AxumStatus::OK, Json(chain)))
}

async fn delete_temporal_fact(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    match state.temporal.delete_fact(&id) {
        true => Ok((AxumStatus::OK, Json(serde_json::json!({"deleted": true})))),
        false => Err(AppError::NotFound(format!("Temporal fact '{}' not found", id))),
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Context Management
// ══════════════════════════════════════════════════════════════════════════

async fn upsert_context_block(
    State(state): State<ApiState>,
    Json(body): Json<ContextBlockBody>,
) -> Result<impl IntoResponse, AppError> {
    let mut ctx = state.context.write().await;        let content = body.content.clone();
    let block = ContextBlock {
        block_id: body.block_id,
        label: body.label,
        content: body.content,
        pinned: body.pinned,
        priority: body.priority.unwrap_or(50),
        max_tokens: body.max_tokens.unwrap_or(1024),
        current_tokens: content.split_whitespace().count(),
        last_updated: chrono::Utc::now().to_rfc3339(),
        metadata: std::collections::HashMap::new(),
    };
    ctx.upsert_block(block)
        .map_err(|e| AppError::Internal(format!("Failed: {}", e)))?;
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({"updated": true})),
    ))
}

async fn get_context_block(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = state.context.read().await;
    match ctx.get_block(&id) {
        Some(b) => Ok((AxumStatus::OK, Json(b.clone()))),
        None => Err(AppError::NotFound(format!("Context block '{}' not found", id))),
    }
}

async fn list_context_blocks(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = state.context.read().await;
    let blocks: Vec<ContextBlock> = ctx.active_blocks().values().cloned().collect();
    Ok((AxumStatus::OK, Json(blocks)))
}

async fn delete_context_block(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut ctx = state.context.write().await;
    if ctx.remove_block(&id).is_none() {
        return Err(AppError::NotFound(format!("Context block '{}' not found", id)));
    }
    let _ = state.store.delete_context_block(&id);
    Ok((AxumStatus::OK, Json(serde_json::json!({"deleted": true}))))
}

async fn render_context(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = state.context.read().await;
    let rendered = ctx.render_context();
    let usage = ctx.token_usage();
    let budget = ctx.token_budget();
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({
            "context": rendered,
            "token_usage": usage,
            "token_budget": budget,
            "max_tokens": ctx.config().max_tokens,
        })),
    ))
}

async fn search_context_summaries(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(50);
    let summaries = state.context.read().await.search_summaries(&query.q, limit);
    Ok((AxumStatus::OK, Json(summaries)))
}

async fn list_context_summaries(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let summaries = state.context.read().await.list_summaries(limit);
    Ok((AxumStatus::OK, Json(summaries)))
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Smart Search & Health
// ══════════════════════════════════════════════════════════════════════════

async fn smart_search(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(10).min(100);
    match state.store.search_fts(&query.q, limit) {
        Ok(results) => {
            let search_results: Vec<SearchResult> = results
                .into_iter()
                .map(|(record, rank)| SearchResult {
                    record,
                    score: (1.0 / (1.0 + rank.abs())).clamp(0.0, 1.0),
                    method: "fts_smart".into(),
                })
                .collect();
            Ok((AxumStatus::OK, Json(search_results)))
        }
        Err(e) => Err(AppError::Database(format!("Smart search failed: {}", e))),
    }
}

async fn system_health(State(state): State<ApiState>) -> Result<impl IntoResponse, AppError> {
    let stats = state
        .store
        .stats()
        .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
    let graph_edges = state
        .graph
        .edge_count()
        .map_err(|e| AppError::Database(format!("Failed: {}", e)))?;
    let mut recommendations = Vec::new();
    for (tier_name, tier_stats) in &stats.tier_breakdown {
        if tier_stats.total_records > 0 && tier_stats.average_importance < 0.3 {
            recommendations.push(format!(
                "Consider reviewing {} tier records (low avg importance: {:.2})",
                tier_name, tier_stats.average_importance
            ));
        }
    }
    Ok((
        AxumStatus::OK,
        Json(serde_json::json!({
            "status": "ok",
            "total_records": stats.total_records,
            "total_across_tiers": stats.total_records,
            "graph_edges": graph_edges,
            "recommendations": recommendations,
        })),
    ))
}

// ══════════════════════════════════════════════════════════════════════════
//  HANDLERS — Stats & Clear
// ══════════════════════════════════════════════════════════════════════════

async fn get_stats(State(state): State<ApiState>) -> Result<impl IntoResponse, AppError> {
    match state.store.stats() {
        Ok(stats) => Ok((AxumStatus::OK, Json(stats))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn clear_all(State(state): State<ApiState>) -> Result<impl IntoResponse, AppError> {
    match state.store.clear() {
        Ok(()) => Ok((AxumStatus::OK, Json(serde_json::json!({"cleared": true})))),
        Err(e) => Err(AppError::Database(format!("Failed: {}", e))),
    }
}

async fn metrics_handler() -> impl IntoResponse {
    let body = crate::metrics::render_prometheus();
    (
        AxumStatus::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

// ══════════════════════════════════════════════════════════════════════════
//  API SERVER
// ══════════════════════════════════════════════════════════════════════════

/// The Memory HTTP API server.
pub struct MemoryApi {
    pub state: ApiState,
    pub addr: String,
}

impl MemoryApi {
    pub fn new(db_path: &str, addr: &str) -> Result<Self, String> {
        Self::with_embedder(db_path, addr, None)
    }

    pub fn with_embedder(
        db_path: &str,
        addr: &str,
        embedder: Option<Arc<dyn Embedder>>,
    ) -> Result<Self, String> {
        // Dynamic vector dimension from embedder or env var
        let vector_dimension = embedder
            .as_ref()
            .map(|e| e.dimension())
            .or_else(|| {
                std::env::var("MEMORY_VECTOR_DIM")
                    .ok()
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(768);

        let config = StorageConfig {
            db_path: db_path.to_string(),
            max_ram_entries: 100,
            auto_embed: false,
            vector_dimension,
        };
        let store = MemoryStore::open(&config).map_err(|e| format!("DB error: {}", e))?;
        let tiered = TieredMemory {
            store: store.clone(),
            working: crate::tiers::WorkingMemory::new(100, 3600),
            promotion: crate::tiers::PromotionEngine::new(),
        };
        let graph = Arc::new(KnowledgeGraph::new(store.clone()));
        let reasoning = Arc::new(ReasoningEngine::new(store.clone()));
        let consolidation = Arc::new(ConsolidationEngine::new(store.clone()));
        let evo_config = EvolutionConfig::default();
        let evolution = EvolutionEngine::new(store.clone(), evo_config);
        let reflection = Arc::new(ReflectionEngine::new(store.clone()));
        let temporal = Arc::new(TemporalEngine::with_defaults(store.clone()));
        let context = Arc::new(RwLock::new(ContextManager::with_defaults(store.clone())));

        Ok(Self {
            state: ApiState {
                store,
                embedder,
                tiered: Arc::new(RwLock::new(tiered)),
                graph,
                reasoning,
                consolidation,
                evolution: Arc::new(RwLock::new(evolution)),
                reflection,
                temporal,
                context,
            },
            addr: addr.to_string(),
        })
    }

    pub fn router(&self) -> Router {
        let default_timeout_secs: u64 = std::env::var("MEMORY_REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);

        Router::new()
            .route("/records", post(insert_record).get(list_records))
            .route("/records/:id", get(get_record).delete(delete_record))
            .route("/search", get(search_records))
            .route("/search/semantic", post(search_semantic))
            .route("/search/smart", get(smart_search))
            .route("/embed", post(embed_text))
            .route("/tiers/:tier", get(list_by_tier))
            .route("/tiers/promote/:id/:target_tier", post(promote_record))
            .route("/tiers/flush", post(flush_working))
            .route("/tiers/auto-promote/:tier", post(run_auto_promotion))
            .route("/graph/edges", post(add_graph_edge))
            .route("/graph/edges/:record_id", get(get_graph_edges))
            .route("/graph/bfs", post(bfs_graph))
            .route("/graph/related/:record_id", get(get_related_records))
            .route("/graph/hubs", get(graph_hubs))
            .route("/reason", post(reason_about))
            .route("/reason/chains/:chain_id", get(get_reasoning_chain))
            .route("/reason/search", get(search_reasoning_chains))
            .route("/reason/distill/:chain_id", post(distill_reasoning_chain))
            .route("/consolidate", post(run_consolidation))
            .route("/consolidate/analyze", get(analyze_tiers))
            .route("/consolidate/conflicts", post(detect_conflicts))
            .route("/evolve", post(run_evolution))
            .route("/evolution/events", get(get_evolution_events))
            .route("/sleep-cycle", post(run_sleep_cycle))
            // Reflections
            .route("/reflections", post(create_reflection).get(list_reflections))
            .route("/reflections/search", get(search_reflections))
            .route("/reflections/:id", get(get_reflection).delete(delete_reflection))
            .route("/assessments", post(run_self_assessment).get(list_assessments))
            .route("/assessments/:id", get(get_assessment))
            .route("/reflexion", post(run_reflexion))
            // Temporal Facts
            .route("/temporal/facts", post(store_temporal_fact).get(list_temporal_facts))
            .route("/temporal/facts/search", get(search_temporal_facts))
            .route("/temporal/facts/:id", get(get_temporal_fact).delete(delete_temporal_fact))
            .route("/temporal/facts/:id/decay", get(temporal_decay_score))
            .route("/temporal/facts/:id/versions", get(get_temporal_version_chain))
            .route("/temporal/recall", post(recall_temporal_fact))
            // Context Management
            .route("/context/blocks", post(upsert_context_block).get(list_context_blocks))
            .route("/context/blocks/:id", get(get_context_block).delete(delete_context_block))
            .route("/context/render", get(render_context))
            .route("/context/summaries", get(list_context_summaries))
            .route("/context/summaries/search", get(search_context_summaries))
            // Namespaces
            .route("/namespaces", post(create_namespace).get(list_namespaces))
            .route("/namespaces/:id", get(get_namespace).delete(delete_namespace))
            .route("/namespaces/:id/records", get(list_records_in_namespace))
            .route("/namespaces/:id/search", get(search_in_namespace))
            .route("/namespaces/:id/stats", get(namespace_stats))
            .route("/namespaces/name/:name", get(get_namespace_by_name))
            .route("/namespaces/check-read", post(check_namespace_access))
            .route("/health", get(system_health))
            .route("/stats", get(get_stats))
            .route("/clear", post(clear_all))
            .route("/metrics", get(metrics_handler))
            .layer(CorsLayer::permissive())
            .with_state(self.state.clone())
            .layer(middleware::from_fn(logging_middleware))
            .layer(middleware::from_fn(auth_middleware))
            .layer(TimeoutLayer::new(Duration::from_secs(default_timeout_secs)))
    }

    pub async fn serve(self) -> Result<(), String> {
        let listener = tokio::net::TcpListener::bind(&self.addr)
            .await
            .map_err(|e| format!("Failed to bind {}: {}", self.addr, e))?;

        let router = self.router();
        let has_embedder = self.state.embedder.is_some();

        println!(" agentic-memory API listening on http://{}", self.addr);
        println!("   POST /records          -- Insert a record (with tier)");
        println!("   GET  /records          -- List records");
        println!("   GET  /records/:id      -- Get record by ID");
        println!("   DELETE /records/:id    -- Delete record");
        println!("   GET  /search?q=&tier=  -- Full-text search (optionally by tier)");
        println!("   POST /search/semantic  -- Semantic (vector) search");
        println!("   GET  /search/smart?q=  -- Smart search (FTS + importance ranking)");
        if has_embedder {
            println!("   POST /embed            -- Embed text via configured provider");
        }
        println!();
        println!("   --- Tier Operations ---");
        println!("   GET  /tiers/:tier      -- List records in a tier");
        println!("   POST /tiers/promote/:id/:tier -- Promote record to tier");
        println!("   POST /tiers/flush      -- Flush working memory");
        println!("   POST /tiers/auto-promote/:tier -- Run auto-promotion");
        println!();
        println!("   --- Knowledge Graph ---");
        println!("   POST /graph/edges      -- Add edge");
        println!("   GET  /graph/edges/:id  -- Get edges for record");
        println!("   POST /graph/bfs        -- BFS traversal");
        println!("   GET  /graph/related/:id -- Get related records");
        println!();
        println!("   --- Reasoning ---");
        println!("   POST /reason           -- Run chain-of-thought reasoning");
        println!("   GET  /reason/chains/:id-- Get reasoning chain");
        println!("   GET  /reason/search?q= -- Search reasoning chains");
        println!("   POST /reason/distill/:id-- Distill chain to procedure");
        println!();
        println!("   --- Consolidation ---");
        println!("   POST /consolidate      -- Run consolidation cycle");
        println!("   GET  /consolidate/analyze -- Analyze tier health");
        println!("   POST /consolidate/conflicts -- Detect conflicts");
        println!();
        println!("   --- Evolution & Sleep ---");
        println!("   POST /evolve           -- Run evolution cycle");
        println!("   GET  /evolution/events -- Get evolution events");
        println!("   POST /sleep-cycle      -- Run full sleep cycle (consolidate + tune + prune + reflexion)");
        println!();
        println!("   --- Reflections ---");
        println!("   POST /reflections      -- Create a reflection");
        println!("   GET  /reflections      -- List reflections");
        println!("   GET  /assessments      -- List self-assessments");
        println!("   POST /assessments      -- Run self-assessment");
        println!("   POST /reflexion        -- Run reflexion loop");
        println!();
        println!("   --- Temporal Facts ---");
        println!("   POST /temporal/facts   -- Store temporal fact");
        println!("   GET  /temporal/facts   -- List temporal facts");
        println!("   POST /temporal/recall  -- Recall a temporal fact");
        println!();
        println!("   --- Context Management ---");
        println!("   POST /context/blocks   -- Add/update context block");
        println!("   GET  /context/render   -- Render context window");
        println!();
        println!("   --- System ---");
        println!("   GET  /health           -- System health overview");
        println!("   GET  /stats            -- Storage statistics");
        println!("   --- Namespaces ---");
        println!("   POST /namespaces       -- Create namespace");
        println!("   GET  /namespaces       -- List namespaces");
        println!("   GET  /namespaces/:id   -- Get namespace by ID");
        println!("   GET  /namespaces/name/:name -- Get namespace by name");
        println!("   POST /namespaces/check-read -- Check read access between namespaces");
        println!();
        println!("   --- System ---");
        println!("   POST /clear            -- Clear all records");
        println!("   GET  /metrics          -- Prometheus metrics");

        if has_embedder {
            println!();
            println!("   Embedder ready");
        }

        axum::serve(listener, router)
            .await
            .map_err(|e| format!("Server error: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn test_stats_endpoint() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_insert_record() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();

        let body = serde_json::json!({
            "id": "api-test-1",
            "content": "API test content",
            "content_type": "test",
            "tier": "episodic",
            "importance": 0.8
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/records")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::CREATED);
    }

    #[tokio::test]
    async fn test_list_by_tier() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/tiers/episodic")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_smart_search() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/search/smart?q=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_search_endpoint() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/search?q=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_search_by_tier() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/search?q=test&tier=episodic")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_list_records() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/records")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_list_records_by_type() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/records?type=news")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_get_record_not_found() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/records/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_record_not_found() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/records/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_add_graph_edge() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();

        for id in ["node-a", "node-b"] {
            let body = serde_json::json!({
                "id": id,
                "content": format!("Node {}", id),
                "content_type": "graph_test",
            });
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/records")
                        .header("Content-Type", "application/json")
                        .body(Body::from(serde_json::to_string(&body).unwrap()))
                        .unwrap(),
                )
                .await;
        }

        let body = serde_json::json!({
            "source_id": "node-a",
            "target_id": "node-b",
            "relation_type": "related_to",
            "weight": 0.9
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/graph/edges")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::CREATED);
    }

    #[tokio::test]
    async fn test_get_graph_edges_empty() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/graph/edges/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_bfs_graph() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let body = serde_json::json!({
            "start_id": "node-a",
            "max_depth": 2
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/graph/bfs")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_graph_hubs() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/graph/hubs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_reason_about() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let body = serde_json::json!({
            "goal": "Analyze market trend",
            "context_query": "bitcoin price",
            "tags": ["market", "analysis"]
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/reason")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_search_reasoning_chains() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/reason/search?q=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_run_consolidation() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/consolidate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_analyze_tiers() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/consolidate/analyze")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_detect_conflicts() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let body = serde_json::json!({"content_type": "fact"});
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/consolidate/conflicts")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_run_evolution() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/evolve")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_get_evolution_events() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/evolution/events?q=")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_flush_working() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tiers/flush")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_auto_promotion() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tiers/auto-promote/episodic")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_clear_all() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/clear")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn test_metrics() {
        let api = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap();
        let app = api.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
    }

    // ══════════════════════════════════════════════════════════════════════
    //  INTEGRATION TESTS — Full Pipeline
    // ══════════════════════════════════════════════════════════════════════

    /// Helper: insert a record via the API and return the response body.
    async fn api_insert(
        app: &Router,
        id: &str,
        content: &str,
        content_type: &str,
        tier: &str,
        importance: f64,
    ) -> serde_json::Value {
        let body = serde_json::json!({
            "id": id,
            "content": content,
            "content_type": content_type,
            "tier": tier,
            "importance": importance,
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/records")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::CREATED);
        serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    /// Helper: GET request, return (status, body JSON).
    async fn api_get(app: &Router, uri: &str) -> (AxumStatus, serde_json::Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null));
        (status, body)
    }

    /// Helper: POST request with JSON body, return (status, body JSON).
    async fn api_post(
        app: &Router,
        uri: &str,
        body: serde_json::Value,
    ) -> (AxumStatus, serde_json::Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null));
        (status, body)
    }

    /// Helper: POST request with no body, return (status, body JSON).
    async fn api_post_empty(app: &Router, uri: &str) -> (AxumStatus, serde_json::Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null));
        (status, body)
    }

    // ── Pipeline 1: Insert → Search → Get → Delete ─────────────────────

    #[tokio::test]
    async fn integration_insert_search_get_delete() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(
            &app,
            "doc-1",
            "Rust is a systems programming language",
            "note",
            "episodic",
            0.8,
        )
        .await;
        api_insert(
            &app,
            "doc-2",
            "Python is great for data science",
            "note",
            "episodic",
            0.7,
        )
        .await;
        api_insert(
            &app,
            "doc-3",
            "Rust ownership prevents data races",
            "fact",
            "semantic",
            0.9,
        )
        .await;

        let (status, results) = api_get(&app, "/search?q=Rust&limit=5").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(results.is_array());
        let arr = results.as_array().unwrap();
        assert!(
            arr.len() >= 2,
            "Expected at least 2 Rust results, got {}",
            arr.len()
        );

        let (status, record) = api_get(&app, "/records/doc-1").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(record["record"]["id"], "doc-1");
        assert_eq!(
            record["record"]["content"],
            "Rust is a systems programming language"
        );
        assert_eq!(record["tier"], "Episodic");

        let (status, records) = api_get(&app, "/records").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(records.is_array());
        assert!(records.as_array().unwrap().len() >= 3);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/records/doc-2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);

        let (status, _) = api_get(&app, "/records/doc-2").await;
        assert_eq!(status, AxumStatus::NOT_FOUND);
    }

    // ── Pipeline 2: Insert → Search by Tier → Promote ──────────────────

    #[tokio::test]
    async fn integration_tier_lifecycle() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(
            &app,
            "ep-1",
            "Yesterday I visited the office",
            "event",
            "episodic",
            0.6,
        )
        .await;
        api_insert(
            &app,
            "ep-2",
            "The meeting was productive",
            "event",
            "episodic",
            0.4,
        )
        .await;
        api_insert(
            &app,
            "sem-1",
            "Rust uses ownership for memory safety",
            "fact",
            "semantic",
            0.9,
        )
        .await;
        api_insert(
            &app,
            "proc-1",
            "Always run cargo clippy before commit",
            "procedure",
            "procedural",
            0.85,
        )
        .await;

        let (status, episodic) = api_get(&app, "/tiers/episodic").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(episodic.as_array().unwrap().len() >= 2);

        let (status, semantic) = api_get(&app, "/tiers/semantic").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(semantic.as_array().unwrap().len() >= 1);

        let (status, body) = api_post_empty(&app, "/tiers/promote/ep-1/semantic").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(body["promoted"], "ep-1");
        assert_eq!(body["to"], "semantic");

        let (status, record) = api_get(&app, "/records/ep-1").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(record["tier"], "Semantic");

        let (status, body) = api_post_empty(&app, "/tiers/auto-promote/episodic").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(body["auto_promoted"].is_number());
    }

    // ── Pipeline 3: Graph — Insert → Edge → BFS → Related ─────────────

    #[tokio::test]
    async fn integration_graph_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(
            &app,
            "concept-A",
            "Machine Learning",
            "concept",
            "semantic",
            0.8,
        )
        .await;
        api_insert(
            &app,
            "concept-B",
            "Neural Networks",
            "concept",
            "semantic",
            0.75,
        )
        .await;
        api_insert(
            &app,
            "concept-C",
            "Deep Learning",
            "concept",
            "semantic",
            0.9,
        )
        .await;
        api_insert(
            &app,
            "concept-D",
            "Natural Language Processing",
            "concept",
            "semantic",
            0.7,
        )
        .await;

        api_post(
            &app,
            "/graph/edges",
            serde_json::json!({
                "source_id": "concept-A",
                "target_id": "concept-B",
                "relation_type": "includes",
                "weight": 0.9
            }),
        )
        .await;

        api_post(
            &app,
            "/graph/edges",
            serde_json::json!({
                "source_id": "concept-B",
                "target_id": "concept-C",
                "relation_type": "includes",
                "weight": 0.85
            }),
        )
        .await;

        api_post(
            &app,
            "/graph/edges",
            serde_json::json!({
                "source_id": "concept-C",
                "target_id": "concept-D",
                "relation_type": "applies_to",
                "weight": 0.7
            }),
        )
        .await;

        let (status, edges) = api_get(&app, "/graph/edges/concept-A").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(edges.is_array());
        assert!(edges.as_array().unwrap().len() >= 1);

        let (status, _bfs) = api_post(
            &app,
            "/graph/bfs",
            serde_json::json!({
                "start_id": "concept-A",
                "max_depth": 3
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);

        let (status, related) = api_get(&app, "/graph/related/concept-A").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(related.is_array());

        let (status, hubs) = api_get(&app, "/graph/hubs?limit=5").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(hubs.is_array());
    }

    // ── Pipeline 4: Reasoning — Create Chain → Search ──────────────────

    #[tokio::test]
    async fn integration_reasoning_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(
            &app,
            "ctx-1",
            "Bitcoin dropped 10% yesterday due to regulatory news",
            "event",
            "episodic",
            0.7,
        )
        .await;
        api_insert(
            &app,
            "ctx-2",
            "The SEC announced new crypto regulations",
            "fact",
            "semantic",
            0.8,
        )
        .await;

        let (status, chain) = api_post(
            &app,
            "/reason",
            serde_json::json!({
                "goal": "Understand why Bitcoin dropped",
                "context_query": "Bitcoin regulatory SEC",
                "tags": ["crypto", "analysis"]
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);
        assert!(chain.is_object());
        let chain_id = chain["chain_id"].as_str().unwrap().to_string();

        assert!(
            chain_id.starts_with("chain_"),
            "chain_id should start with 'chain_', got: {}",
            chain_id
        );

        let (status, chains) = api_get(&app, "/reason/search?q=Bitcoin").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(chains.is_array());
    }

    // ── Pipeline 5: Consolidation — Insert Duplicates → Consolidate ────

    #[tokio::test]
    async fn integration_consolidation_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(
            &app,
            "dup-1",
            "Rust uses ownership for memory safety",
            "fact",
            "semantic",
            0.8,
        )
        .await;
        api_insert(
            &app,
            "dup-2",
            "Rust ownership ensures memory safety",
            "fact",
            "semantic",
            0.75,
        )
        .await;
        api_insert(
            &app,
            "dup-3",
            "Go uses garbage collection for memory management",
            "fact",
            "semantic",
            0.7,
        )
        .await;

        let (status, report) = api_post_empty(&app, "/consolidate").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(report.is_object());

        let (status, analyses) = api_get(&app, "/consolidate/analyze").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(analyses.is_array());
        assert!(analyses.as_array().unwrap().len() == 4);

        let (status, conflicts) = api_post(
            &app,
            "/consolidate/conflicts",
            serde_json::json!({
                "content_type": "fact"
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);
        assert!(conflicts.is_array());
    }

    // ── Pipeline 6: Evolution — Insert → Tune → Prune ─────────────────

    #[tokio::test]
    async fn integration_evolution_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        for i in 0..5 {
            api_insert(
                &app,
                &format!("evo-{}", i),
                &format!("Evolution test record number {}", i),
                "test",
                "episodic",
                0.3 + (i as f64 * 0.1),
            )
            .await;
        }

        let (status, body) = api_post_empty(&app, "/evolve").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(body["tuning_events"].is_number());

        let (status, events) = api_get(&app, "/evolution/events?q=").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(events.is_array());
    }

    // ── Pipeline 7: Full Lifecycle ─────────────────────────────────────

    #[tokio::test]
    async fn integration_full_lifecycle() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(
            &app,
            "life-1",
            "Alice joined the team in January",
            "event",
            "episodic",
            0.6,
        )
        .await;
        api_insert(
            &app,
            "life-2",
            "Alice is a senior engineer",
            "fact",
            "semantic",
            0.8,
        )
        .await;
        api_insert(
            &app,
            "life-3",
            "Alice led the migration project",
            "event",
            "episodic",
            0.7,
        )
        .await;
        api_insert(
            &app,
            "life-4",
            "The migration project reduced costs by 30%",
            "fact",
            "semantic",
            0.85,
        )
        .await;
        api_insert(
            &app,
            "life-5",
            "Always backup before database migrations",
            "procedure",
            "procedural",
            0.9,
        )
        .await;

        api_post(
            &app,
            "/graph/edges",
            serde_json::json!({
                "source_id": "life-2",
                "target_id": "life-1",
                "relation_type": "participates_in",
                "weight": 0.8
            }),
        )
        .await;
        api_post(
            &app,
            "/graph/edges",
            serde_json::json!({
                "source_id": "life-3",
                "target_id": "life-4",
                "relation_type": "causes",
                "weight": 0.9
            }),
        )
        .await;

        let (status, results) = api_get(&app, "/search?q=Alice").await;
        assert_eq!(status, AxumStatus::OK);
        let alice_results = results.as_array().unwrap();
        assert!(
            alice_results.len() >= 2,
            "Expected at least 2 Alice results, got {}",
            alice_results.len()
        );

        let (status, _) = api_post_empty(&app, "/tiers/promote/life-4/semantic").await;
        assert_eq!(status, AxumStatus::OK);

        let (status, _chain) = api_post(
            &app,
            "/reason",
            serde_json::json!({
                "goal": "Summarize Alice's contributions",
                "context_query": "Alice migration",
                "tags": ["team", "projects"]
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);

        let (status, _) = api_post_empty(&app, "/consolidate").await;
        assert_eq!(status, AxumStatus::OK);

        let (status, _) = api_post_empty(&app, "/evolve").await;
        assert_eq!(status, AxumStatus::OK);

        let (status, health) = api_get(&app, "/health").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(health["status"], "ok");
        assert!(health["total_records"].as_u64().unwrap() >= 5);
        assert!(health["graph_edges"].as_u64().unwrap() >= 2);

        let (status, stats) = api_get(&app, "/stats").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(stats["total_records"].as_u64().unwrap() >= 5);

        let (status, analyses) = api_get(&app, "/consolidate/analyze").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(analyses.as_array().unwrap().len(), 4);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            content_type.contains("text/plain"),
            "Expected text/plain for Prometheus, got {}",
            content_type
        );
    }

    // ── Pipeline 8: Flush Working Memory ───────────────────────────────

    #[tokio::test]
    async fn integration_working_memory_flush() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(
            &app,
            "wm-1",
            "Transient observation about weather",
            "observation",
            "working",
            0.5,
        )
        .await;

        let (status, body) = api_post_empty(&app, "/tiers/flush").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(body["flushed"].is_number());

        let (status, record) = api_get(&app, "/records/wm-1").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(record["tier"], "Episodic");
    }

    // ── Pipeline 9: Clear and Verify ───────────────────────────────────

    #[tokio::test]
    async fn integration_clear_and_verify() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        api_insert(&app, "clr-1", "Data to be cleared", "test", "episodic", 0.5).await;
        api_insert(&app, "clr-2", "More data to clear", "test", "semantic", 0.6).await;

        let (status, health) = api_get(&app, "/health").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(health["total_records"].as_u64().unwrap() >= 2);

        let (status, body) = api_post_empty(&app, "/clear").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(body["cleared"], true);

        let (status, health) = api_get(&app, "/health").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(health["total_records"].as_u64().unwrap(), 0);
    }

    // ══════════════════════════════════════════════════════════════════════
    //  INTEGRATION TESTS — Reflection, Temporal, Context, Sleep Cycle
    // ══════════════════════════════════════════════════════════════════════

    // ── Pipeline 10: Reflection → Assessment → Reflexion Loop ──────────

    #[tokio::test]
    async fn integration_reflection_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        // Insert some records so the assessment has data to analyze
        for i in 0..3 {
            api_insert(
                &app,
                &format!("ref-{}", i),
                &format!("Reflection test record {}", i),
                "note",
                "episodic",
                0.5 + (i as f64 * 0.1),
            )
            .await;
        }

        // Create a reflection
        let (status, reflection) = api_post(
            &app,
            "/reflections",
            serde_json::json!({
                "topic": "memory quality",
                "monologue": "Looking at my current state, things seem stable.",
                "conclusion": "Memory is in good shape.",
                "planned_actions": ["continue monitoring"],
                "confidence": 0.8,
                "tags": ["self-assessment"]
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::CREATED);
        assert!(reflection["reflection_id"].as_str().unwrap().starts_with("refl_"));
        assert_eq!(reflection["topic"], "memory quality");
        assert!(reflection["confidence"].as_f64().unwrap() > 0.5);

        // List reflections
        let (status, reflections) = api_get(&app, "/reflections").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(reflections.is_array());
        assert!(reflections.as_array().unwrap().len() >= 1);

        // Search reflections
        let (status, results) = api_get(&app, "/reflections/search?q=memory").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(results.is_array());
        assert!(results.as_array().unwrap().len() >= 1);

        // Run self-assessment
        let (status, assessment) = api_post_empty(&app, "/assessments").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(assessment["overall_health"].as_f64().unwrap() >= 0.0);
        assert!(assessment["memory_quality_score"].as_f64().unwrap() >= 0.0);

        // List assessments
        let (status, assessments) = api_get(&app, "/assessments").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(assessments.is_array());
        assert!(assessments.as_array().unwrap().len() >= 1);

        // Run reflexion loop
        let (status, result) = api_post(
            &app,
            "/reflexion",
            serde_json::json!({
                "topic": "system health",
                "observation": "Starting a new session with 3 records"
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);
        assert!(result["reflection"].is_object());
        assert!(result["assessment"].is_object());
        assert!(result["reflection"]["topic"].as_str().unwrap() == "system health");

        // Delete a reflection
        let refl_id = reflection["reflection_id"].as_str().unwrap();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/reflections/{}", refl_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);

        // Verify deletion
        let (status, _) = api_get(&app, &format!("/reflections/{}", refl_id)).await;
        assert_eq!(status, AxumStatus::NOT_FOUND);
    }

    // ── Pipeline 11: Temporal Facts — Store → Recall → Decay → Versions

    #[tokio::test]
    async fn integration_temporal_fact_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        // Store a temporal fact
        let (status, fact) = api_post(
            &app,
            "/temporal/facts",
            serde_json::json!({
                "content": "The Earth orbits the Sun",
                "content_type": "fact",
                "importance": 0.9
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::CREATED);
        let fact_id = fact["fact_id"].as_str().unwrap().to_string();
        assert!(fact_id.starts_with("fact_"));
        assert_eq!(fact["content"], "The Earth orbits the Sun");
        assert_eq!(fact["version"], 1);
        assert!(fact["decay_score"].as_f64().unwrap() > 0.9);

        // Get the fact
        let (status, retrieved) = api_get(&app, &format!("/temporal/facts/{}", fact_id)).await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(retrieved["content"], "The Earth orbits the Sun");

        // List temporal facts
        let (status, facts) = api_get(&app, "/temporal/facts").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(facts.is_array());
        assert!(facts.as_array().unwrap().len() >= 1);

        // Search temporal facts
        let (status, results) = api_get(&app, "/temporal/facts/search?q=Earth").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(results.is_array());
        assert!(results.as_array().unwrap().len() >= 1);

        // Recall the fact (boost decay)
        let (status, recalled) = api_post(
            &app,
            "/temporal/recall",
            serde_json::json!({
                "fact_id": fact_id
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(recalled["recall_count"], 1);
        assert!(recalled["decay_score"].as_f64().unwrap() > 0.9);

        // Check decay score
        let (status, decay_info) = api_get(&app, &format!("/temporal/facts/{}/decay", fact_id)).await;
        assert_eq!(status, AxumStatus::OK);
        assert!(decay_info["decay_score"].as_f64().unwrap() > 0.0);
        assert!(decay_info["effective_importance"].as_f64().unwrap() > 0.0);
        assert_eq!(decay_info["is_stale"], false);

        // Get version chain
        let (status, versions) = api_get(&app, &format!("/temporal/facts/{}/versions", fact_id)).await;
        assert_eq!(status, AxumStatus::OK);
        assert!(versions.is_array());
        assert!(versions.as_array().unwrap().len() >= 1);

        // Store a second fact for search testing
        api_post(
            &app,
            "/temporal/facts",
            serde_json::json!({
                "content": "Mars is the fourth planet",
                "content_type": "fact",
                "importance": 0.7
            }),
        )
        .await;

        // List should show both
        let (status, facts) = api_get(&app, "/temporal/facts").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(facts.as_array().unwrap().len() >= 2);

        // Delete the fact
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/temporal/facts/{}", fact_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);

        // Verify deletion
        let (status, _) = api_get(&app, &format!("/temporal/facts/{}", fact_id)).await;
        assert_eq!(status, AxumStatus::NOT_FOUND);
    }

    // ── Pipeline 12: Context Management — Blocks → Render → Evict → Summary

    #[tokio::test]
    async fn integration_context_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        // Render initial context (should have persona + user blocks)
        let (status, rendered) = api_get(&app, "/context/render").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(rendered["context"].as_str().unwrap().contains("PERSONA"));
        assert!(rendered["token_usage"].as_u64().unwrap() > 0);
        assert!(rendered["max_tokens"].as_u64().unwrap() > 0);

        // List context blocks
        let (status, blocks) = api_get(&app, "/context/blocks").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(blocks.is_array());
        assert!(blocks.as_array().unwrap().len() >= 2); // persona + user

        // Add a new context block
        let (status, body) = api_post(
            &app,
            "/context/blocks",
            serde_json::json!({
                "block_id": "recent",
                "label": "recent",
                "content": "Recent conversation context with important details",
                "pinned": false,
                "priority": 50
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(body["updated"], true);

        // Get the block
        let (status, block) = api_get(&app, "/context/blocks/recent").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(block["label"], "recent");
        assert!(block["content"].as_str().unwrap().contains("Recent conversation"));

        // Render context should now include the new block
        let (status, rendered) = api_get(&app, "/context/render").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(rendered["context"].as_str().unwrap().contains("RECENT"));

        // Add another block to test eviction
        api_post(
            &app,
            "/context/blocks",
            serde_json::json!({
                "block_id": "extra",
                "label": "extra",
                "content": "Extra context that might cause eviction",
                "pinned": false,
                "priority": 10
            }),
        )
        .await;

        // Delete a context block
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/context/blocks/extra")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);

        // Verify deletion
        let (status, _) = api_get(&app, "/context/blocks/extra").await;
        assert_eq!(status, AxumStatus::NOT_FOUND);
    }

    // ── Pipeline 13: Sleep Cycle — Full Evolution Pipeline ──────────────

    #[tokio::test]
    async fn integration_sleep_cycle_pipeline() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        // Insert records across multiple tiers
        for i in 0..5 {
            api_insert(
                &app,
                &format!("sleep-{}", i),
                &format!("Sleep cycle test record {}", i),
                "test",
                "episodic",
                0.3 + (i as f64 * 0.1),
            )
            .await;
        }
        api_insert(
            &app,
            "sleep-proc",
            "Always run tests before deploy",
            "procedure",
            "procedural",
            0.9,
        )
        .await;

        // Run sleep cycle
        let (status, report) = api_post_empty(&app, "/sleep-cycle").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(report.is_object());
        assert!(report["cycle_id"].as_str().unwrap().starts_with("sleep_"));
        assert!(report["duration_ms"].as_u64().is_some());

        // Should have consolidation report
        assert!(report["consolidation_report"].is_object());
        assert!(
            report["consolidation_report"]["records_processed"]
                .as_u64()
                .unwrap()
                >= 5
        );

        // Should have reflexion results
        assert!(report["reflection"].is_object());
        assert!(report["assessment"].is_object());
        assert!(
            report["reflection"]["topic"]
                .as_str()
                .unwrap()
                == "sleep_cycle"
        );
        assert!(report["assessment"]["overall_health"].as_f64().unwrap() >= 0.0);

        // System should still be healthy after sleep cycle
        let (status, health) = api_get(&app, "/health").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(health["status"], "ok");
    }

    // ── Pipeline 14: Full Universal Self-Evolving Pipeline ──────────────

    #[tokio::test]
    async fn integration_universal_self_evolution() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        // Phase 1: Store memories across all tiers
        api_insert(&app, "ue-1", "Alice joined in January", "event", "episodic", 0.6).await;
        api_insert(&app, "ue-2", "Alice is a senior engineer", "fact", "semantic", 0.8).await;
        api_insert(&app, "ue-3", "Always backup before deploy", "procedure", "procedural", 0.9).await;

        // Phase 2: Store temporal facts
        let (status, fact) = api_post(
            &app,
            "/temporal/facts",
            serde_json::json!({
                "content": "Project deadline is Friday",
                "content_type": "event",
                "importance": 0.7
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::CREATED);
        let fact_id = fact["fact_id"].as_str().unwrap();

        // Phase 3: Add context blocks
        api_post(
            &app,
            "/context/blocks",
            serde_json::json!({
                "block_id": "project",
                "label": "project_context",
                "content": "Working on the memory system upgrade",
                "pinned": false,
                "priority": 60
            }),
        )
        .await;

        // Phase 4: Create reflection
        api_post(
            &app,
            "/reflections",
            serde_json::json!({
                "topic": "project status",
                "monologue": "Making good progress on the memory upgrade.",
                "conclusion": "On track for Friday deadline.",
                "confidence": 0.85
            }),
        )
        .await;

        // Phase 5: Run consolidation
        let (status, _) = api_post_empty(&app, "/consolidate").await;
        assert_eq!(status, AxumStatus::OK);

        // Phase 6: Run reflexion
        let (status, result) = api_post(
            &app,
            "/reflexion",
            serde_json::json!({
                "topic": "project progress",
                "observation": "All systems operational"
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);
        assert!(result["reflection"].is_object());
        assert!(result["assessment"].is_object());

        // Phase 7: Recall temporal fact
        let (status, recalled) = api_post(
            &app,
            "/temporal/recall",
            serde_json::json!({"fact_id": fact_id}),
        )
        .await;
        assert_eq!(status, AxumStatus::OK);
        assert!(recalled["recall_count"].as_u64().unwrap() >= 1);

        // Phase 8: Render context
        let (status, ctx) = api_get(&app, "/context/render").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(ctx["context"].as_str().unwrap().contains("PROJECT_CONTEXT"));

        // Phase 9: Run evolution
        let (status, _) = api_post_empty(&app, "/evolve").await;
        assert_eq!(status, AxumStatus::OK);

        // Phase 10: Run full sleep cycle
        let (status, report) = api_post_empty(&app, "/sleep-cycle").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(report["reflection"].is_object());
        assert!(report["assessment"].is_object());

        // Phase 11: Final health check
        let (status, health) = api_get(&app, "/health").await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(health["status"], "ok");
        assert!(health["total_records"].as_u64().unwrap() >= 3);

        // Phase 12: Stats
        let (status, stats) = api_get(&app, "/stats").await;
        assert_eq!(status, AxumStatus::OK);
        assert!(stats["total_records"].as_u64().unwrap() >= 3);
    }

    // ── Pipeline 15: Namespace Validation — Reject Non-Existent Namespace ══

    #[tokio::test]
    async fn integration_namespace_validation_rejects_nonexistent() {
        let app = MemoryApi::new(":memory:", "0.0.0.0:0").unwrap().router();

        // 1) Inserting into a non-existent namespace should return 404
        let body = serde_json::json!({
            "id": "ns-1",
            "content": "Record in a ghost namespace",
            "content_type": "note",
            "tier": "episodic",
            "namespace_id": "nonexistent_ns"
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/records")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::NOT_FOUND);

        // 2) Inserting without namespace_id (defaults to "default") should succeed
        let body = serde_json::json!({
            "id": "ns-default-1",
            "content": "Record in default namespace",
            "content_type": "note",
            "tier": "episodic"
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/records")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::CREATED);

        // 3) Create a namespace, then insert into it — should succeed
        let (status, ns) = api_post(
            &app,
            "/namespaces",
            serde_json::json!({
                "name": "agent-alpha",
                "owner": "alice",
                "description": "Alice's private namespace"
            }),
        )
        .await;
        assert_eq!(status, AxumStatus::CREATED);
        let ns_id = ns["namespace_id"].as_str().unwrap().to_string();

        let body = serde_json::json!({
            "id": "ns-alpha-1",
            "content": "Record in Alice's namespace",
            "content_type": "note",
            "tier": "episodic",
            "namespace_id": ns_id
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/records")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::CREATED);

        // 4) Verify the record appears in the namespace
        let (status, records) = api_get(&app, &format!("/namespaces/{}/records", ns_id)).await;
        assert_eq!(status, AxumStatus::OK);
        assert_eq!(records.as_array().unwrap().len(), 1);
        assert_eq!(records.as_array().unwrap()[0]["id"], "ns-alpha-1");

        // 5) Delete the namespace, then try inserting into it — should 404 again
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/namespaces/{}", ns_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::OK);

        let body = serde_json::json!({
            "id": "ns-alpha-2",
            "content": "Record in deleted namespace",
            "content_type": "note",
            "tier": "episodic",
            "namespace_id": ns_id
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/records")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), AxumStatus::NOT_FOUND);
    }
}