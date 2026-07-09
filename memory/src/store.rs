//! # Memory Store — SQLite-Backed Hierarchical Memory Storage
//!
//! Extended with tier-aware storage, knowledge graph, reasoning chains,
//! expert opinions, evolution tracking, and bitemporal metadata.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};

use crate::migrations;
use crate::types::{
    ContextBlock, ContextSummary, GraphEdge, GraphTraversalResult, MemoryRecord,
    MemoryStats, MemoryTier, Namespace, ReasoningChain, ReasoningStep, Reflection,
    SelfAssessment, StorageConfig, TemporalFact, TierConfig, TierStats, TieredRecord,
};

/// Initialize the sqlite-vec extension globally via sqlite3_auto_extension.
/// Safe to call multiple times — subsequent calls are no-ops at the SQLite level.
#[allow(clippy::missing_transmute_annotations)]
pub fn init_vector_search() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

/// SQLite-backed store with tier-aware, graph, reasoning, and vector search support.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    pub(crate) pool: Arc<Pool<SqliteConnectionManager>>,
    vector_dimension: usize,
    vector_search_enabled: Arc<AtomicBool>,
}

impl MemoryStore {
    /// Acquire the database connection lock.
    /// Converts a poisoned mutex into a `rusqlite::Error` instead of panicking.
    ///
    /// **WARNING:** `std::sync::Mutex` is not re-entrant. Any method that holds
    /// the returned `MutexGuard` must NOT call other methods that acquire `lock_db()`.
    /// Use block scopes `{ let conn = self.lock_db()?; ... }` to drop the guard
    /// before calling lock-acquiring methods, or use `get_tier_config_with_conn()`
    /// variants that accept an already-held `&Connection`.
    /// Acquire a connection from the pool.
    pub fn lock_db(&self) -> rusqlite::Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool.get().map_err(|e| {
            rusqlite::Error::InvalidParameterName(format!("Pool connection failed: {}", e))
        })
    }

    pub fn open(config: &StorageConfig) -> rusqlite::Result<Self> {
        init_vector_search();

        // Create connection manager with WAL pragmas applied to each connection
        let manager = if config.db_path == ":memory:" {
            SqliteConnectionManager::memory()
        } else {
            SqliteConnectionManager::file(&config.db_path)
        };

        // Build pool: 8 concurrent connections, 2 kept warm
        let pool = Pool::builder()
            .max_size(8)
            .min_idle(Some(2))
            .build(manager)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Pool build failed: {}", e)))?;

        let dim = config.vector_dimension.clamp(64, 4096);

        // Apply pragmas to the pool by configuring the first connection
        {
            let conn = pool.get().map_err(|e| {
                rusqlite::Error::InvalidParameterName(format!("Failed to get initial connection: {}", e))
            })?;
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA busy_timeout=5000;
                 PRAGMA foreign_keys=ON;
                 PRAGMA temp_store=MEMORY;",
            )?;
        }

        let store = Self {
            pool: Arc::new(pool),
            vector_dimension: dim,
            vector_search_enabled: Arc::new(AtomicBool::new(false)),
        };
        store.initialize_tables()?;
        Ok(store)
    }

    fn initialize_tables(&self) -> rusqlite::Result<()> {
        // Run versioned migrations (creates all tables and indexes)
        self.run_migrations()?;
        
        // Initialize vector tables
        {
            let conn = self.lock_db()?;
            let _ = self.init_vector_tables(&conn);
        }

        Ok(())
    }

    /// Run database schema migrations.
    /// This ensures the database schema is always up-to-date when the application starts.
    fn run_migrations(&self) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        migrations::run_migrations(&conn)?;
        Ok(())
    }

    fn init_vector_tables(&self, conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vector_map (
                record_id TEXT PRIMARY KEY,
                vec_rowid INTEGER NOT NULL
            );",
        )?;

        let dim = self.vector_dimension;
        let sql = format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vectors_ann USING vec0(
                embedding float[{}] distance_metric=cosine
            )",
            dim
        );

        match conn.execute_batch(&sql) {
            Ok(()) => {
                self.vector_search_enabled.store(true, Ordering::Relaxed);
                eprintln!("✅ sqlite-vec: vector search initialized (dim={})", dim);
                Ok(())
            }
            Err(e) => {
                eprintln!(
                    "ℹ️  sqlite-vec not available: {} — using fallback linear scan",
                    e
                );
                Ok(())
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    //  TIER-AWARE CRUD
    // ══════════════════════════════════════════════════════════════════════

    pub fn insert_into_tier(
        &self,
        record: &MemoryRecord,
        tier: MemoryTier,
        importance: f64,
        ttl_seconds: Option<u64>,
        parent_id: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.insert_into_tier_with_namespace(record, tier, importance, ttl_seconds, parent_id, "default")
    }

    /// Insert a record into a specific tier with optional namespace isolation.
    pub fn insert_into_tier_with_namespace(
        &self,
        record: &MemoryRecord,
        tier: MemoryTier,
        importance: f64,
        ttl_seconds: Option<u64>,
        parent_id: Option<&str>,
        namespace_id: &str,
    ) -> rusqlite::Result<()> {
        // Scope the lock so it's released before calling store_embedding,
        // which also acquires the lock.
        {
            let mut conn = self.lock_db()?;

            // Use a transaction for atomicity across records + FTS
            let tx = conn.transaction()?;

            let metadata_json = serde_json::to_string(&record.metadata).unwrap_or_default();
            let embedding_blob: Option<Vec<u8>> = record
                .embedding
                .as_ref()
                .map(|v| v.iter().flat_map(|f| f.to_le_bytes()).collect());

            let now = chrono::Utc::now().to_rfc3339();

            // 1. Insert into main records table
            tx.execute(
                "INSERT OR REPLACE INTO records
                 (id, content, content_type, metadata_json, embedding, timestamp,
                  tier, importance, access_count, last_accessed, ttl_seconds, parent_id,
                  valid_from, sys_start, namespace_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, ?9, ?9, ?12)",
                params![
                    record.id,
                    record.content,
                    record.content_type,
                    metadata_json,
                    embedding_blob,
                    record.timestamp,
                    tier.to_string(),
                    importance,
                    now,
                    ttl_seconds.map(|v| v as i64),
                    parent_id,
                    namespace_id,
                ],
            )?;

            // 2. Insert into FTS index
            tx.execute(
                "INSERT OR REPLACE INTO records_fts (id, content, content_type, metadata_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    record.id,
                    record.content,
                    record.content_type,
                    metadata_json
                ],
            )?;

            // Commit the main transaction (records + FTS)
            tx.commit()?;
        } // MutexGuard dropped here — safe to call store_embedding now

        // 3. Handle vector embedding with compensation for data integrity
        if let Some(ref emb) = record.embedding {
            if let Err(e) = self.store_embedding(&record.id, emb) {
                tracing::error!(
                    "Vector embedding failed for {}. Attempting compensation delete.",
                    record.id
                );
                let _ = self.delete(&record.id);
                return Err(e);
            }
        }

        Ok(())
    }

    pub fn insert(&self, record: &MemoryRecord) -> rusqlite::Result<()> {
        self.insert_into_tier(record, MemoryTier::Episodic, 0.5, None, None)
    }

    pub fn get_tiered(&self, id: &str) -> rusqlite::Result<Option<TieredRecord>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, content_type, metadata_json, embedding, timestamp,
                    tier, importance, access_count, last_accessed, ttl_seconds, parent_id
             FROM records WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_tiered_record)?;
        match rows.next() {
            Some(Ok(record)) => {
                // Best-effort update of access stats. Failure here should not fail the get.
                if let Err(e) = conn.execute(
                    "UPDATE records SET access_count = access_count + 1, last_accessed = ?1 WHERE id = ?2",
                    params![chrono::Utc::now().to_rfc3339(), id],
                ) {
                    tracing::warn!("Failed to update access stats for {}: {}", id, e);
                }
                Ok(Some(record))
            }
            _ => Ok(None),
        }
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<MemoryRecord>> {
        self.get_tiered(id).map(|opt| opt.map(|t| t.record))
    }

    pub fn delete(&self, id: &str) -> rusqlite::Result<bool> {
        let mut conn = self.lock_db()?;

        // Wrap multi-step delete in a transaction for data integrity
        let tx = conn.transaction()?;

        // 1. Get vector rowid if exists
        let rowid_opt: Option<i64> = tx
            .query_row(
                "SELECT vec_rowid FROM vector_map WHERE record_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .ok();

        if let Some(vec_rowid) = rowid_opt {
            let _ = tx.execute(
                "DELETE FROM vectors_ann WHERE rowid = ?1",
                params![vec_rowid],
            );
            let _ = tx.execute("DELETE FROM vector_map WHERE record_id = ?1", params![id]);
        }

        // 2. Delete from main tables
        let deleted = tx.execute("DELETE FROM records WHERE id = ?1", params![id])?;
        let _ = tx.execute("DELETE FROM records_fts WHERE id = ?1", params![id]);
        let _ = tx.execute(
            "DELETE FROM graph_edges WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        );

        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn list_by_type(
        &self,
        content_type: &str,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<MemoryRecord>> {
        self.list_by_type_tiered(content_type, limit, offset)
            .map(|v| v.into_iter().map(|t| t.record).collect())
    }

    pub fn list_by_type_tiered(
        &self,
        content_type: &str,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<TieredRecord>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, content_type, metadata_json, embedding, timestamp,
                    tier, importance, access_count, last_accessed, ttl_seconds, parent_id
             FROM records WHERE content_type = ?1
             ORDER BY importance DESC, timestamp DESC LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map(
            params![content_type, limit as i64, offset as i64],
            row_to_tiered_record,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn list_by_tier(
        &self,
        tier: MemoryTier,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<TieredRecord>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, content_type, metadata_json, embedding, timestamp,
                    tier, importance, access_count, last_accessed, ttl_seconds, parent_id
             FROM records WHERE tier = ?1
             ORDER BY importance DESC, timestamp DESC LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map(
            params![tier.to_string(), limit as i64, offset as i64],
            row_to_tiered_record,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn all_with_embeddings(&self) -> rusqlite::Result<Vec<MemoryRecord>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, content_type, metadata_json, embedding, timestamp
             FROM records WHERE embedding IS NOT NULL
             ORDER BY timestamp DESC",
        )?;

        let rows = stmt.query_map([], row_to_record)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn all(&self, limit: usize, offset: usize) -> rusqlite::Result<Vec<MemoryRecord>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, content_type, metadata_json, embedding, timestamp
             FROM records ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![limit as i64, offset as i64], row_to_record)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ── Promotion / Demotion ─────────────────────────────────────────────

    pub fn promote(&self, id: &str, to_tier: MemoryTier) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let updated = conn.execute(
            "UPDATE records SET tier = ?1 WHERE id = ?2",
            params![to_tier.to_string(), id],
        )?;
        Ok(updated > 0)
    }

    pub fn update_importance(&self, id: &str, importance: f64) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let updated = conn.execute(
            "UPDATE records SET importance = ?1 WHERE id = ?2",
            params![importance, id],
        )?;
        Ok(updated > 0)
    }

    pub fn get_eviction_candidates(
        &self,
        tier: MemoryTier,
        count: usize,
    ) -> rusqlite::Result<Vec<TieredRecord>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, content_type, metadata_json, embedding, timestamp,
                    tier, importance, access_count, last_accessed, ttl_seconds, parent_id
             FROM records WHERE tier = ?1
             ORDER BY importance ASC, access_count ASC, timestamp ASC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(
            params![tier.to_string(), count as i64],
            row_to_tiered_record,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn evict_from_tier(&self, tier: MemoryTier, max_to_keep: usize) -> rusqlite::Result<u64> {
        let mut conn = self.lock_db()?;
        let tier_str = tier.to_string();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM records WHERE tier = ?1",
            params![tier_str],
            |r| r.get(0),
        )?;

        if count <= max_to_keep as i64 {
            return Ok(0);
        }

        let to_evict = (count - max_to_keep as i64) as u64;

        // Wrap in transaction to ensure atomicity across records, FTS, and vector cleanup
        let tx = conn.transaction()?;
        {
            // 1. Clean up vector data for records being evicted
            tx.execute(
                "DELETE FROM vectors_ann WHERE rowid IN (
                    SELECT vm.vec_rowid FROM vector_map vm
                    JOIN records r ON r.id = vm.record_id
                    WHERE r.tier = ?1
                    ORDER BY r.importance ASC, r.access_count ASC, r.timestamp ASC
                    LIMIT ?2
                )",
                params![tier_str, to_evict as i64],
            )?;
            tx.execute(
                "DELETE FROM vector_map WHERE record_id IN (
                    SELECT id FROM records WHERE tier = ?1
                    ORDER BY importance ASC, access_count ASC, timestamp ASC
                    LIMIT ?2
                )",
                params![tier_str, to_evict as i64],
            )?;

            // 2. Delete the records themselves
            tx.execute(
                "DELETE FROM records WHERE id IN (
                    SELECT id FROM records WHERE tier = ?1
                    ORDER BY importance ASC, access_count ASC, timestamp ASC
                    LIMIT ?2
                )",
                params![tier_str, to_evict as i64],
            )?;

            // 3. Clean up orphaned FTS entries
            tx.execute(
                "DELETE FROM records_fts WHERE id NOT IN (SELECT id FROM records)",
                [],
            )?;
        }
        tx.commit()?;
        Ok(to_evict)
    }

    // ── Full-Text Search ─────────────────────────────────────────────────

    pub fn search_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<(MemoryRecord, f64)>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT r.id, r.content, r.content_type, r.metadata_json, r.embedding, r.timestamp,
                    rank
             FROM records_fts f
             JOIN records r ON r.id = f.id
             WHERE records_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![query, limit as i64], |row| {
            let record = row_to_record(row)?;
            let rank: f64 = row.get(6)?;
            Ok((record, rank))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn search_fts_in_tier(
        &self,
        query: &str,
        tier: MemoryTier,
        limit: usize,
    ) -> rusqlite::Result<Vec<(MemoryRecord, f64)>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT r.id, r.content, r.content_type, r.metadata_json, r.embedding, r.timestamp,
                    rank
             FROM records_fts f
             JOIN records r ON r.id = f.id
             WHERE records_fts MATCH ?1 AND r.tier = ?2
             ORDER BY rank
             LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![query, tier.to_string(), limit as i64], |row| {
            let record = row_to_record(row)?;
            let rank: f64 = row.get(6)?;
            Ok((record, rank))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ── Tier Configuration ───────────────────────────────────────────────

    /// Read tier config from an already-held connection (no extra lock).
    fn get_tier_config_with_conn(&self, conn: &Connection, tier: MemoryTier) -> TierConfig {
        conn.query_row(
            "SELECT max_records, default_ttl_secs, promotion_threshold, demotion_threshold, auto_promote
             FROM tier_config WHERE tier = ?1",
            params![tier.to_string()],
            |row| {
                Ok(TierConfig {
                    max_records: row.get::<_, i64>(0)? as usize,
                    default_ttl_seconds: row.get::<_, Option<i64>>(1)?.map(|v| v as u64),
                    promotion_threshold: row.get(2)?,
                    demotion_threshold: row.get(3)?,
                    auto_promote: row.get::<_, i32>(4)? != 0,
                })
            },
        ).unwrap_or_else(|_| TierConfig::for_tier(tier))
    }

    pub fn get_tier_config(&self, tier: MemoryTier) -> rusqlite::Result<TierConfig> {
        let conn = self.lock_db()?;
        Ok(self.get_tier_config_with_conn(&conn, tier))
    }

    pub fn update_tier_config(
        &self,
        tier: MemoryTier,
        config: &TierConfig,
    ) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        conn.execute(
            "INSERT OR REPLACE INTO tier_config
             (tier, max_records, default_ttl_secs, promotion_threshold, demotion_threshold, auto_promote)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                tier.to_string(),
                config.max_records as i64,
                config.default_ttl_seconds.map(|s| s as i64),
                config.promotion_threshold,
                config.demotion_threshold,
                config.auto_promote as i32,
            ],
        )?;
        Ok(())
    }

    // ══════════════════════════════════════════════════════════════════════
    //  KNOWLEDGE GRAPH OPERATIONS
    // ══════════════════════════════════════════════════════════════════════

    pub fn add_edge(
        &self,
        source_id: &str,
        target_id: &str,
        relation_type: &str,
        weight: f64,
    ) -> rusqlite::Result<String> {
        let conn = self.lock_db()?;
        let edge_id = format!("edge_{}", crate::generate_id());
        conn.execute(
            "INSERT INTO graph_edges (edge_id, source_id, target_id, relation_type, weight)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![edge_id, source_id, target_id, relation_type, weight],
        )?;
        Ok(edge_id)
    }

    pub fn get_edges(&self, record_id: &str) -> rusqlite::Result<Vec<GraphEdge>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT edge_id, source_id, target_id, relation_type, weight, metadata_json, created_at
             FROM graph_edges WHERE source_id = ?1 OR target_id = ?1
             ORDER BY weight DESC",
        )?;

        let rows = stmt.query_map(params![record_id], row_to_edge)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn graph_bfs(
        &self,
        start_id: &str,
        max_depth: u32,
        relation_filter: Option<&str>,
    ) -> rusqlite::Result<Vec<GraphTraversalResult>> {
        let conn = self.lock_db()?;

        if max_depth == 0 {
            return Ok(vec![GraphTraversalResult {
                node_id: start_id.to_string(),
                depth: 0,
                path: vec![start_id.to_string()],
                cumulative_weight: 1.0,
            }]);
        }

        // Use parameterized query for relation_filter to prevent SQL injection
        let (sql, extra_param): (String, Option<String>) = match relation_filter {
            Some(rel) => (
                "WITH RECURSIVE traversal(node_id, depth, path, cum_weight) AS (
                    SELECT ?1, 0, ?1, 1.0
                    UNION
                    SELECT e.target_id, t.depth + 1,
                           t.path || ',' || e.target_id,
                           t.cum_weight * e.weight
                    FROM traversal t
                    JOIN graph_edges e ON e.source_id = t.node_id
                    WHERE t.depth < ?2 AND e.relation_type = ?3
                )
                SELECT DISTINCT node_id, depth, path, cum_weight FROM traversal
                ORDER BY depth, cum_weight DESC"
                    .to_string(),
                Some(rel.to_string()),
            ),
            None => (
                "WITH RECURSIVE traversal(node_id, depth, path, cum_weight) AS (
                    SELECT ?1, 0, ?1, 1.0
                    UNION
                    SELECT e.target_id, t.depth + 1,
                           t.path || ',' || e.target_id,
                           t.cum_weight * e.weight
                    FROM traversal t
                    JOIN graph_edges e ON e.source_id = t.node_id
                    WHERE t.depth < ?2
                )
                SELECT DISTINCT node_id, depth, path, cum_weight FROM traversal
                ORDER BY depth, cum_weight DESC"
                    .to_string(),
                None,
            ),
        };

        let mut stmt = conn.prepare(&sql)?;

        // Build parameterized list to avoid duplicate closures
        let mut params_list: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(start_id.to_string()), Box::new(max_depth)];
        if let Some(rel) = &extra_param {
            params_list.push(Box::new(rel.clone()));
        }
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_list.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(GraphTraversalResult {
                node_id: row.get(0)?,
                depth: row.get(1)?,
                path: row
                    .get::<_, String>(2)?
                    .split(',')
                    .map(|s| s.to_string())
                    .collect(),
                cumulative_weight: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn get_related_records(
        &self,
        record_id: &str,
        relation_type: Option<&str>,
        max_depth: u32,
    ) -> rusqlite::Result<Vec<(MemoryRecord, u32, String, f64)>> {
        let traversal = self.graph_bfs(record_id, max_depth, relation_type)?;

        let mut results = Vec::new();
        for t in &traversal {
            if t.node_id == record_id {
                continue;
            }
            if let Some(record) = self.get(&t.node_id)? {
                let path_str = t.path.join(" → ");
                results.push((record, t.depth, path_str, t.cumulative_weight));
            }
        }
        Ok(results)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  REASONING CHAINS
    // ══════════════════════════════════════════════════════════════════════

    pub fn store_reasoning_chain(&self, chain: &ReasoningChain) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        let steps_json = serde_json::to_string(&chain.steps).unwrap_or_default();
        let consulted_json = serde_json::to_string(&chain.consulted_records).unwrap_or_default();
        let tags_json = serde_json::to_string(&chain.tags).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO reasoning_chains
             (chain_id, goal, steps_json, final_conclusion, overall_confidence, success,
              consulted_records, tags_json, created_at, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                chain.chain_id,
                chain.goal,
                steps_json,
                chain.final_conclusion,
                chain.overall_confidence,
                chain.success as i32,
                consulted_json,
                tags_json,
                chain.created_at,
                chain.duration_ms as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_reasoning_chain(&self, chain_id: &str) -> rusqlite::Result<Option<ReasoningChain>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT chain_id, goal, steps_json, final_conclusion, overall_confidence, success,
                    consulted_records, tags_json, created_at, duration_ms
             FROM reasoning_chains WHERE chain_id = ?1",
        )?;

        let mut rows = stmt.query_map(params![chain_id], row_to_reasoning_chain)?;
        match rows.next() {
            Some(Ok(chain)) => Ok(Some(chain)),
            _ => Ok(None),
        }
    }

    pub fn search_reasoning_chains(
        &self,
        goal_query: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<ReasoningChain>> {
        let conn = self.lock_db()?;
        let pattern = format!("%{}%", goal_query);
        let mut stmt = conn.prepare(
            "SELECT chain_id, goal, steps_json, final_conclusion, overall_confidence, success,
                    consulted_records, tags_json, created_at, duration_ms
             FROM reasoning_chains
             WHERE goal LIKE ?1 OR final_conclusion LIKE ?1
             ORDER BY overall_confidence DESC, created_at DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![pattern, limit as i64], row_to_reasoning_chain)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  EXPERT OPINIONS
    // ══════════════════════════════════════════════════════════════════════

    #[allow(clippy::too_many_arguments)]
    pub fn store_opinion(
        &self,
        opinion_id: &str,
        expert_type: &str,
        target_record_id: Option<&str>,
        recommendation: &str,
        reasoning: &str,
        confidence: f64,
        action_taken: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        conn.execute(
            "INSERT INTO expert_opinions (opinion_id, expert_type, target_record_id, recommendation, reasoning, confidence, action_taken)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![opinion_id, expert_type, target_record_id, recommendation, reasoning, confidence, action_taken],
        )?;
        Ok(())
    }

    pub fn get_opinions_by_expert(
        &self,
        expert_type: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<(String, String, f64, String)>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT recommendation, reasoning, confidence, created_at
             FROM expert_opinions WHERE expert_type = ?1
             ORDER BY confidence DESC, created_at DESC LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![expert_type, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  EVOLUTION EVENTS
    // ══════════════════════════════════════════════════════════════════════

    pub fn record_evolution_event(
        &self,
        event_id: &str,
        event_type: &str,
        description: &str,
        previous_value: Option<&str>,
        new_value: Option<&str>,
        confidence: f64,
    ) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        conn.execute(
            "INSERT INTO evolution_events (event_id, event_type, description, previous_value, new_value, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![event_id, event_type, description, previous_value, new_value, confidence],
        )?;
        Ok(())
    }

    pub fn get_evolution_events(
        &self,
        event_type: Option<&str>,
        limit: usize,
    ) -> rusqlite::Result<Vec<(String, String, f64, String)>> {
        let conn = self.lock_db()?;
        let (sql, type_param): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(et) = event_type {
                (
                    "SELECT event_type, description, confidence, timestamp FROM evolution_events
                 WHERE event_type = ?1 ORDER BY timestamp DESC LIMIT ?2"
                        .to_string(),
                    vec![Box::new(et.to_string()), Box::new(limit as i64)],
                )
            } else {
                (
                    "SELECT event_type, description, confidence, timestamp FROM evolution_events
                 ORDER BY timestamp DESC LIMIT ?1"
                        .to_string(),
                    vec![Box::new(limit as i64)],
                )
            };

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            type_param.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  STATISTICS
    // ══════════════════════════════════════════════════════════════════════

    pub fn stats(&self) -> rusqlite::Result<MemoryStats> {
        let conn = self.lock_db()?;

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))
            .unwrap_or(0);
        let with_embeddings: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM records WHERE embedding IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut stmt =
            conn.prepare("SELECT content_type, COUNT(*) FROM records GROUP BY content_type")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        let mut content_types = HashMap::new();
        for row in rows {
            let (ct, cnt) = row?;
            content_types.insert(ct, cnt);
        }

        let page_count: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap_or(0);
        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .unwrap_or(0);

        // Batch-fetch all tier configs in one query (avoids N+1 pattern)
        let mut all_tier_configs: HashMap<String, TierConfig> = HashMap::new();
        {
            let mut cfg_stmt = conn.prepare(
                "SELECT tier, max_records, default_ttl_secs, promotion_threshold, demotion_threshold, auto_promote
                 FROM tier_config",
            )?;
            let cfg_rows = cfg_stmt.query_map([], |row| {
                let tier_name: String = row.get(0)?;
                Ok((
                    tier_name,
                    TierConfig {
                        max_records: row.get::<_, i64>(1)? as usize,
                        default_ttl_seconds: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                        promotion_threshold: row.get(3)?,
                        demotion_threshold: row.get(4)?,
                        auto_promote: row.get::<_, i32>(5)? != 0,
                    },
                ))
            })?;
            for row in cfg_rows {
                let (name, cfg) = row?;
                all_tier_configs.insert(name, cfg);
            }
        }

        let mut tier_breakdown = HashMap::new();
        for tier_str in ["working", "episodic", "semantic", "procedural"] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM records WHERE tier = ?1",
                    params![tier_str],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let emb: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM records WHERE tier = ?1 AND embedding IS NOT NULL",
                    params![tier_str],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let avg_imp: f64 = conn
                .query_row(
                    "SELECT COALESCE(AVG(importance), 0.0) FROM records WHERE tier = ?1",
                    params![tier_str],
                    |r| r.get(0),
                )
                .unwrap_or(0.0);
            let accesses: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(access_count), 0) FROM records WHERE tier = ?1",
                    params![tier_str],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            let tier = match tier_str {
                "working" => MemoryTier::Working,
                "episodic" => MemoryTier::Episodic,
                "semantic" => MemoryTier::Semantic,
                "procedural" => MemoryTier::Procedural,
                _ => unreachable!(),
            };

            let config = all_tier_configs
                .remove(tier_str)
                .unwrap_or_else(|| TierConfig::for_tier(tier));

            tier_breakdown.insert(
                tier_str.to_string(),
                TierStats {
                    tier,
                    total_records: count as u64,
                    total_with_embeddings: emb as u64,
                    average_importance: avg_imp,
                    total_accesses: accesses as u64,
                    storage_bytes: 0,
                    config,
                },
            );
        }

        Ok(MemoryStats {
            total_records: total as u64,
            total_with_embeddings: with_embeddings as u64,
            content_types,
            storage_bytes: (page_count * page_size) as u64,
            tier_breakdown,
        })
    }

    // ══════════════════════════════════════════════════════════════════════
    //  VECTOR SEARCH (sqlite-vec)
    // ══════════════════════════════════════════════════════════════════════

    pub fn store_embedding(&self, record_id: &str, embedding: &[f64]) -> rusqlite::Result<()> {
        if !self.vector_search_enabled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let mut conn = self.lock_db()?;

        // Wrap vector operations in a transaction
        let tx = conn.transaction()?;

        let old_rowid: Option<i64> = tx
            .query_row(
                "SELECT vec_rowid FROM vector_map WHERE record_id = ?1",
                params![record_id],
                |r| r.get::<_, i64>(0),
            )
            .ok();

        if let Some(old_rowid) = old_rowid {
            let _ = tx.execute(
                "DELETE FROM vectors_ann WHERE rowid = ?1",
                params![old_rowid],
            );
            let _ = tx.execute(
                "DELETE FROM vector_map WHERE record_id = ?1",
                params![record_id],
            );
        }

        let f32_bytes: Vec<u8> = embedding
            .iter()
            .map(|&v| v as f32)
            .flat_map(|f| f.to_le_bytes())
            .collect();

        tx.execute(
            "INSERT INTO vectors_ann(embedding) VALUES (?1)",
            params![f32_bytes],
        )?;

        let vec_rowid = tx.last_insert_rowid();

        tx.execute(
            "INSERT INTO vector_map(record_id, vec_rowid) VALUES (?1, ?2)",
            params![record_id, vec_rowid],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn search_vectors(
        &self,
        query: &[f64],
        k: usize,
    ) -> rusqlite::Result<Vec<(MemoryRecord, f32)>> {
        if !self.vector_search_enabled.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }

        let conn = self.lock_db()?;

        let f32_bytes: Vec<u8> = query
            .iter()
            .map(|&v| v as f32)
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let mut stmt = conn.prepare(
            "SELECT r.id, r.content, r.content_type, r.metadata_json, r.embedding, r.timestamp,
                    v.distance
             FROM (
                 SELECT rowid, distance
                 FROM vectors_ann
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2
             ) v
             JOIN vector_map m ON m.vec_rowid = v.rowid
             JOIN records r ON r.id = m.record_id
             ORDER BY v.distance",
        )?;

        let rows = stmt.query_map(params![f32_bytes, k as i64], |row| {
            let record = row_to_record(row)?;
            let distance: f32 = row.get(6)?;
            Ok((record, distance))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    pub fn search_vectors_hybrid(
        &self,
        query: &[f64],
        k: usize,
        top_n: usize,
    ) -> rusqlite::Result<Vec<(MemoryRecord, f32)>> {
        if !self.vector_search_enabled.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        let candidates = self.search_vectors(query, top_n)?;

        if candidates.is_empty() || candidates.len() <= k {
            return Ok(candidates.into_iter().take(k).collect());
        }

        let query_binary = crate::vector::quantize_binary(query);

        let mut reranked: Vec<(f64, MemoryRecord, f32)> = candidates
            .into_iter()
            .filter_map(|(record, cosine_dist)| {
                let emb = record.embedding.as_ref()?;
                let bin = crate::vector::quantize_binary(emb);
                let hamming_sim = crate::vector::hamming_similarity(&query_binary, &bin);
                let combined = hamming_sim - (cosine_dist as f64) / 2.0;
                Some((combined, record, cosine_dist))
            })
            .collect();

        reranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(reranked
            .into_iter()
            .take(k)
            .map(|(_, rec, dist)| (rec, dist))
            .collect())
    }

    pub fn remove_edges_for(&self, record_id: &str) -> rusqlite::Result<u64> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM graph_edges WHERE source_id = ?1 OR target_id = ?1",
            [record_id],
        )?;
        Ok(deleted as u64)
    }

    pub fn clear_graph(&self) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        conn.execute("DELETE FROM graph_edges", [])?;
        Ok(())
    }

    pub fn graph_edge_count(&self) -> rusqlite::Result<u64> {
        let conn = self.lock_db()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |r| r.get(0))?;
        Ok(count as u64)
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        conn.execute_batch(
            "DELETE FROM records;
             DELETE FROM records_fts;
             DELETE FROM graph_edges;
             DELETE FROM reasoning_chains;
             DELETE FROM expert_opinions;
             DELETE FROM evolution_events;
             DELETE FROM reflections;
             DELETE FROM self_assessments;
             DELETE FROM temporal_facts;
             DELETE FROM context_blocks;
             DELETE FROM context_summaries;
             DELETE FROM vectors_ann;
             DELETE FROM vector_map;
             DELETE FROM namespaces;",
        )?;
        Ok(())
    }

    // ══════════════════════════════════════════════════════════════════════
    //  NAMESPACE OPERATIONS
    // ══════════════════════════════════════════════════════════════════════

    /// Create a new namespace.
    pub fn create_namespace(
        &self,
        namespace_id: &str,
        name: &str,
        description: &str,
        owner: &str,
        read_parents: &[String],
        write_children: &[String],
    ) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        let parents_json = serde_json::to_string(read_parents).unwrap_or_default();
        let children_json = serde_json::to_string(write_children).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO namespaces
             (namespace_id, name, description, owner, read_parents, write_children)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![namespace_id, name, description, owner, parents_json, children_json],
        )?;
        Ok(())
    }

    /// Get a namespace by ID.
    pub fn get_namespace(&self, id: &str) -> rusqlite::Result<Option<Namespace>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT namespace_id, name, description, owner, read_parents, write_children, created_at
             FROM namespaces WHERE namespace_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_namespace)?;
        match rows.next() {
            Some(Ok(ns)) => Ok(Some(ns)),
            _ => Ok(None),
        }
    }

    /// Get a namespace by name.
    pub fn get_namespace_by_name(&self, name: &str) -> rusqlite::Result<Option<Namespace>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT namespace_id, name, description, owner, read_parents, write_children, created_at
             FROM namespaces WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], row_to_namespace)?;
        match rows.next() {
            Some(Ok(ns)) => Ok(Some(ns)),
            _ => Ok(None),
        }
    }

    /// List all namespaces.
    pub fn list_namespaces(&self) -> rusqlite::Result<Vec<Namespace>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT namespace_id, name, description, owner, read_parents, write_children, created_at
             FROM namespaces ORDER BY name",
        )?;
        let rows = stmt.query_map([], row_to_namespace)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Delete a namespace.
    pub fn delete_namespace(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM namespaces WHERE namespace_id = ?1",
            params![id],
        )?;
        Ok(deleted > 0)
    }

    /// List records in a specific namespace.
    pub fn list_by_namespace(
        &self,
        namespace_id: &str,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<MemoryRecord>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, content_type, metadata_json, embedding, timestamp
             FROM records WHERE namespace_id = ?1
             ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(
            params![namespace_id, limit as i64, offset as i64],
            row_to_record,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Search within a specific namespace.
    pub fn search_fts_in_namespace(
        &self,
        query: &str,
        namespace_id: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<(MemoryRecord, f64)>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT r.id, r.content, r.content_type, r.metadata_json, r.embedding, r.timestamp,
                    rank
             FROM records_fts f
             JOIN records r ON r.id = f.id
             WHERE records_fts MATCH ?1 AND r.namespace_id = ?2
             ORDER BY rank
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, namespace_id, limit as i64], |row| {
            let record = row_to_record(row)?;
            let rank: f64 = row.get(6)?;
            Ok((record, rank))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Count records in a namespace.
    pub fn count_by_namespace(&self, namespace_id: &str) -> rusqlite::Result<u64> {
        let conn = self.lock_db()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM records WHERE namespace_id = ?1",
            params![namespace_id],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }

    /// Check if a namespace has read access to another namespace (via read_parents).
    pub fn can_read_namespace(&self, reader_ns: &str, target_ns: &str) -> rusqlite::Result<bool> {
        if reader_ns == target_ns {
            return Ok(true);
        }
        let ns = self.get_namespace(reader_ns)?;
        match ns {
            Some(namespace) => Ok(namespace.read_parents.contains(&target_ns.to_string())),
            None => Ok(false),
        }
    }

    /// Get all unique node IDs from the graph (used by KnowledgeGraph methods).
    pub fn get_all_graph_node_ids(&self) -> rusqlite::Result<Vec<String>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT source_id FROM graph_edges
             UNION
             SELECT DISTINCT target_id FROM graph_edges",
        )?;
        let mut ids = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  REFLECTION PERSISTENCE
    // ══════════════════════════════════════════════════════════════════════

    /// Persist a reflection to the database.
    pub fn store_reflection(&self, reflection: &Reflection) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        let planned_json = serde_json::to_string(&reflection.planned_actions).unwrap_or_default();
        let tags_json = serde_json::to_string(&reflection.tags).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO reflections
             (reflection_id, topic, monologue, conclusion, planned_actions, outcome,
              confidence, tags_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                reflection.reflection_id,
                reflection.topic,
                reflection.monologue,
                reflection.conclusion,
                planned_json,
                reflection.outcome,
                reflection.confidence,
                tags_json,
                reflection.created_at,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a reflection by ID.
    pub fn get_reflection(&self, id: &str) -> rusqlite::Result<Option<Reflection>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT reflection_id, topic, monologue, conclusion, planned_actions, outcome,
                    confidence, tags_json, created_at
             FROM reflections WHERE reflection_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_reflection)?;
        match rows.next() {
            Some(Ok(r)) => Ok(Some(r)),
            _ => Ok(None),
        }
    }

    /// Search reflections by topic.
    pub fn search_reflections(
        &self,
        topic_query: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<Reflection>> {
        let conn = self.lock_db()?;
        let pattern = format!("%{}%", topic_query);
        let mut stmt = conn.prepare(
            "SELECT reflection_id, topic, monologue, conclusion, planned_actions, outcome,
                    confidence, tags_json, created_at
             FROM reflections
             WHERE topic LIKE ?1 OR conclusion LIKE ?1
             ORDER BY confidence DESC, created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], row_to_reflection)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// List recent reflections.
    pub fn list_reflections(&self, limit: usize) -> rusqlite::Result<Vec<Reflection>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT reflection_id, topic, monologue, conclusion, planned_actions, outcome,
                    confidence, tags_json, created_at
             FROM reflections
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_reflection)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Delete a reflection.
    pub fn delete_reflection(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM reflections WHERE reflection_id = ?1",
            params![id],
        )?;
        Ok(deleted > 0)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  SELF-ASSESSMENT PERSISTENCE
    // ══════════════════════════════════════════════════════════════════════

    /// Persist a self-assessment to the database.
    pub fn store_assessment(&self, assessment: &SelfAssessment) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        let issues_json = serde_json::to_string(&assessment.issues_detected).unwrap_or_default();
        let recs_json = serde_json::to_string(&assessment.recommendations).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO self_assessments
             (assessment_id, memory_quality_score, coherence_score, staleness_score,
              diversity_score, overall_health, issues_detected, recommendations, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                assessment.assessment_id,
                assessment.memory_quality_score,
                assessment.coherence_score,
                assessment.staleness_score,
                assessment.diversity_score,
                assessment.overall_health,
                issues_json,
                recs_json,
                assessment.created_at,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a self-assessment by ID.
    pub fn get_assessment(&self, id: &str) -> rusqlite::Result<Option<SelfAssessment>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT assessment_id, memory_quality_score, coherence_score, staleness_score,
                    diversity_score, overall_health, issues_detected, recommendations, created_at
             FROM self_assessments WHERE assessment_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_assessment)?;
        match rows.next() {
            Some(Ok(a)) => Ok(Some(a)),
            _ => Ok(None),
        }
    }

    /// List recent self-assessments.
    pub fn list_assessments(&self, limit: usize) -> rusqlite::Result<Vec<SelfAssessment>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT assessment_id, memory_quality_score, coherence_score, staleness_score,
                    diversity_score, overall_health, issues_detected, recommendations, created_at
             FROM self_assessments
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_assessment)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Delete a self-assessment.
    pub fn delete_assessment(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM self_assessments WHERE assessment_id = ?1",
            params![id],
        )?;
        Ok(deleted > 0)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  TEMPORAL FACT PERSISTENCE
    // ══════════════════════════════════════════════════════════════════════

    /// Persist a temporal fact to the database.
    pub fn store_temporal_fact(&self, fact: &TemporalFact) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        let metadata_json = serde_json::to_string(&fact.metadata).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO temporal_facts
             (fact_id, content, content_type, valid_from, valid_to, sys_start, sys_end,
              version, previous_version_id, decay_score, recall_count, last_recalled,
              importance, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                fact.fact_id,
                fact.content,
                fact.content_type,
                fact.valid_from,
                fact.valid_to,
                fact.sys_start,
                fact.sys_end,
                fact.version as i64,
                fact.previous_version_id,
                fact.decay_score,
                fact.recall_count as i64,
                fact.last_recalled,
                fact.importance,
                metadata_json,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a temporal fact by ID.
    pub fn get_temporal_fact(&self, id: &str) -> rusqlite::Result<Option<TemporalFact>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT fact_id, content, content_type, valid_from, valid_to, sys_start, sys_end,
                    version, previous_version_id, decay_score, recall_count, last_recalled,
                    importance, metadata_json
             FROM temporal_facts WHERE fact_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_temporal_fact)?;
        match rows.next() {
            Some(Ok(f)) => Ok(Some(f)),
            _ => Ok(None),
        }
    }

    /// List current (non-superseded) temporal facts.
    pub fn list_current_temporal_facts(
        &self,
        content_type: Option<&str>,
        limit: usize,
    ) -> rusqlite::Result<Vec<TemporalFact>> {
        let conn = self.lock_db()?;
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match content_type {
            Some(ct) => (
                "SELECT fact_id, content, content_type, valid_from, valid_to, sys_start, sys_end,
                        version, previous_version_id, decay_score, recall_count, last_recalled,
                        importance, metadata_json
                 FROM temporal_facts
                 WHERE sys_end IS NULL AND content_type = ?1
                 ORDER BY importance DESC, sys_start DESC
                 LIMIT ?2"
                    .to_string(),
                vec![Box::new(ct.to_string()), Box::new(limit as i64)],
            ),
            None => (
                "SELECT fact_id, content, content_type, valid_from, valid_to, sys_start, sys_end,
                        version, previous_version_id, decay_score, recall_count, last_recalled,
                        importance, metadata_json
                 FROM temporal_facts
                 WHERE sys_end IS NULL
                 ORDER BY importance DESC, sys_start DESC
                 LIMIT ?1"
                    .to_string(),
                vec![Box::new(limit as i64)],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), row_to_temporal_fact)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get version chain for a temporal fact (follows previous_version_id).
    pub fn get_temporal_version_chain(
        &self,
        fact_id: &str,
    ) -> rusqlite::Result<Vec<TemporalFact>> {
        let mut chain = Vec::new();
        let mut current_id = Some(fact_id.to_string());

        while let Some(id) = current_id {
            if let Some(fact) = self.get_temporal_fact(&id)? {
                current_id = fact.previous_version_id.clone();
                chain.push(fact);
            } else {
                break;
            }
        }

        chain.reverse();
        Ok(chain)
    }

    /// Update decay score for a temporal fact.
    pub fn update_temporal_decay(
        &self,
        fact_id: &str,
        decay_score: f64,
        recall_count: u64,
        last_recalled: &str,
    ) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let updated = conn.execute(
            "UPDATE temporal_facts
             SET decay_score = ?1, recall_count = ?2, last_recalled = ?3
             WHERE fact_id = ?4",
            params![decay_score, recall_count as i64, last_recalled, fact_id],
        )?;
        Ok(updated > 0)
    }

    /// Invalidate a temporal fact (set sys_end).
    pub fn invalidate_temporal_fact(&self, fact_id: &str) -> rusqlite::Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.lock_db()?;
        let updated = conn.execute(
            "UPDATE temporal_facts SET sys_end = ?1, valid_to = ?1 WHERE fact_id = ?2",
            params![now, fact_id],
        )?;
        Ok(updated > 0)
    }

    /// Search temporal facts by content.
    pub fn search_temporal_facts(
        &self,
        query: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<TemporalFact>> {
        let conn = self.lock_db()?;
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT fact_id, content, content_type, valid_from, valid_to, sys_start, sys_end,
                    version, previous_version_id, decay_score, recall_count, last_recalled,
                    importance, metadata_json
             FROM temporal_facts
             WHERE sys_end IS NULL AND (content LIKE ?1 OR content_type LIKE ?1)
             ORDER BY importance DESC, decay_score DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], row_to_temporal_fact)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Delete a temporal fact.
    pub fn delete_temporal_fact(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM temporal_facts WHERE fact_id = ?1",
            params![id],
        )?;
        Ok(deleted > 0)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  CONTEXT BLOCK PERSISTENCE
    // ══════════════════════════════════════════════════════════════════════

    /// Persist a context block to the database.
    pub fn store_context_block(&self, block: &ContextBlock) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        let metadata_json = serde_json::to_string(&block.metadata).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO context_blocks
             (block_id, label, content, pinned, priority, max_tokens, current_tokens,
              last_updated, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                block.block_id,
                block.label,
                block.content,
                block.pinned as i32,
                block.priority,
                block.max_tokens as i64,
                block.current_tokens as i64,
                block.last_updated,
                metadata_json,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a context block by ID.
    pub fn get_context_block(&self, id: &str) -> rusqlite::Result<Option<ContextBlock>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT block_id, label, content, pinned, priority, max_tokens, current_tokens,
                    last_updated, metadata_json
             FROM context_blocks WHERE block_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_context_block)?;
        match rows.next() {
            Some(Ok(b)) => Ok(Some(b)),
            _ => Ok(None),
        }
    }

    /// List all context blocks, ordered by priority descending.
    pub fn list_context_blocks(&self) -> rusqlite::Result<Vec<ContextBlock>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT block_id, label, content, pinned, priority, max_tokens, current_tokens,
                    last_updated, metadata_json
             FROM context_blocks
             ORDER BY priority DESC",
        )?;
        let rows = stmt.query_map([], row_to_context_block)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Delete a context block.
    pub fn delete_context_block(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM context_blocks WHERE block_id = ?1",
            params![id],
        )?;
        Ok(deleted > 0)
    }

    /// Sync all context blocks (batch upsert from in-memory state).
    pub fn sync_context_blocks(&self, blocks: &[ContextBlock]) -> rusqlite::Result<()> {
        let mut conn = self.lock_db()?;
        let tx = conn.transaction()?;
        {
            // Clear existing blocks
            tx.execute("DELETE FROM context_blocks", [])?;
            // Insert all blocks
            for block in blocks {
                let metadata_json =
                    serde_json::to_string(&block.metadata).unwrap_or_default();
                tx.execute(
                    "INSERT INTO context_blocks
                     (block_id, label, content, pinned, priority, max_tokens, current_tokens,
                      last_updated, metadata_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        block.block_id,
                        block.label,
                        block.content,
                        block.pinned as i32,
                        block.priority,
                        block.max_tokens as i64,
                        block.current_tokens as i64,
                        block.last_updated,
                        metadata_json,
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ══════════════════════════════════════════════════════════════════════
    //  CONTEXT SUMMARY PERSISTENCE
    // ══════════════════════════════════════════════════════════════════════

    /// Persist a context summary to the database.
    pub fn store_context_summary(&self, summary: &ContextSummary) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        let source_json = serde_json::to_string(&summary.source_block_ids).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO context_summaries
             (summary_id, topic, summary, source_block_ids, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                summary.summary_id,
                summary.topic,
                summary.summary,
                source_json,
                summary.created_at,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a context summary by ID.
    pub fn get_context_summary(&self, id: &str) -> rusqlite::Result<Option<ContextSummary>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT summary_id, topic, summary, source_block_ids, created_at
             FROM context_summaries WHERE summary_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_context_summary)?;
        match rows.next() {
            Some(Ok(s)) => Ok(Some(s)),
            _ => Ok(None),
        }
    }

    /// Search context summaries by topic.
    pub fn search_context_summaries(
        &self,
        query: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<ContextSummary>> {
        let conn = self.lock_db()?;
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT summary_id, topic, summary, source_block_ids, created_at
             FROM context_summaries
             WHERE topic LIKE ?1 OR summary LIKE ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], row_to_context_summary)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// List recent context summaries.
    pub fn list_context_summaries(&self, limit: usize) -> rusqlite::Result<Vec<ContextSummary>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT summary_id, topic, summary, source_block_ids, created_at
             FROM context_summaries
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_context_summary)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Delete a context summary.
    pub fn delete_context_summary(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM context_summaries WHERE summary_id = ?1",
            params![id],
        )?;
        Ok(deleted > 0)
    }

    // ══════════════════════════════════════════════════════════════════════
    //  STRATEGY RATING PERSISTENCE
    // ══════════════════════════════════════════════════════════════════════

    /// Persist a strategy rating to the database.
    pub fn store_strategy_rating(
        &self,
        id: &str,
        namespace_id: &str,
        confidence_tier: &str,
        raw_importance: f64,
        blended_score: f64,
        sample_size: u64,
    ) -> rusqlite::Result<()> {
        let conn = self.lock_db()?;
        conn.execute(
            "INSERT OR REPLACE INTO strategy_ratings
             (id, namespace_id, confidence_tier, raw_importance, blended_score, sample_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, namespace_id, confidence_tier, raw_importance, blended_score, sample_size as i64],
        )?;
        Ok(())
    }

    /// Get star rating distribution grouped by confidence_tier.
    /// Returns a map of tier name → count.
    pub fn strategy_rating_distribution(&self) -> rusqlite::Result<std::collections::HashMap<String, u64>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT confidence_tier, COUNT(*) FROM strategy_ratings GROUP BY confidence_tier",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        let mut dist = std::collections::HashMap::new();
        for row in rows {
            let (tier, count) = row?;
            dist.insert(tier, count);
        }
        Ok(dist)
    }

    /// Get star rating distribution for a specific namespace.
    pub fn strategy_rating_distribution_for_namespace(
        &self,
        namespace_id: &str,
    ) -> rusqlite::Result<std::collections::HashMap<String, u64>> {
        let conn = self.lock_db()?;
        let mut stmt = conn.prepare(
            "SELECT confidence_tier, COUNT(*) FROM strategy_ratings
             WHERE namespace_id = ?1 GROUP BY confidence_tier",
        )?;
        let rows = stmt.query_map(params![namespace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        let mut dist = std::collections::HashMap::new();
        for row in rows {
            let (tier, count) = row?;
            dist.insert(tier, count);
        }
        Ok(dist)
    }

    /// Check if a strategy rating already exists for a record with the same confidence tier.
    /// Used by consolidation Phase 6 to skip redundant writes.
    pub fn has_strategy_rating(&self, rating_id: &str, confidence_tier: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_db()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM strategy_ratings WHERE id = ?1 AND confidence_tier = ?2",
            params![rating_id, confidence_tier],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Clean up orphaned strategy ratings whose record_id no longer exists.
    /// Called during Phase 5 eviction to prevent table bloat.
    pub fn cleanup_orphaned_ratings(&self) -> rusqlite::Result<u64> {
        let conn = self.lock_db()?;
        let deleted = conn.execute(
            "DELETE FROM strategy_ratings WHERE id NOT IN (
                SELECT 'sr_' || id FROM records
            )",
            [],
        )?;
        Ok(deleted as u64)
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  ROW MAPPING FUNCTIONS
// ══════════════════════════════════════════════════════════════════════════

/// Convert a blob of f64 little-endian bytes into a Vec<f64>.
/// Returns None if the blob length is not a multiple of 8 (malformed data).
fn blob_to_f64_vec(blob: Vec<u8>) -> Option<Vec<f64>> {
    if !blob.len().is_multiple_of(8) {
        return None;
    }
    Some(
        blob.chunks_exact(8)
            .map(|chunk| {
                let arr: [u8; 8] = chunk
                    .try_into()
                    .expect("chunks_exact(8) guarantees 8 bytes");
                f64::from_le_bytes(arr)
            })
            .collect(),
    )
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let embedding_blob: Option<Vec<u8>> = row.get(4)?;
    let embedding = embedding_blob.and_then(blob_to_f64_vec);

    let metadata_json: String = row.get(3)?;
    let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();

    Ok(MemoryRecord {
        id: row.get(0)?,
        content: row.get(1)?,
        content_type: row.get(2)?,
        metadata,
        embedding,
        timestamp: row.get(5)?,
    })
}

fn row_to_tiered_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TieredRecord> {
    let embedding_blob: Option<Vec<u8>> = row.get(4)?;
    let embedding = embedding_blob.and_then(blob_to_f64_vec);

    let metadata_json: String = row.get(3)?;
    let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();

    let tier_str: String = row.get(6)?;
    let tier = tier_str
        .parse::<MemoryTier>()
        .unwrap_or(MemoryTier::Episodic);

    Ok(TieredRecord {
        record: MemoryRecord {
            id: row.get(0)?,
            content: row.get(1)?,
            content_type: row.get(2)?,
            metadata,
            embedding,
            timestamp: row.get(5)?,
        },
        tier,
        importance: row.get(7)?,
        access_count: row.get::<_, i64>(8)? as u64,
        last_accessed: row.get(9)?,
        ttl_seconds: row.get::<_, Option<i64>>(10)?.map(|v| v as u64),
        parent_id: row.get(11)?,
        tags: {
            // Robust tag deserialization with logging on failure
            match row.get::<_, Option<String>>(12) {
                Ok(Some(json)) if !json.is_empty() => match serde_json::from_str(&json) {
                    Ok(tags) => tags,
                    Err(e) => {
                        tracing::warn!("Failed to deserialize tags_json for record: {}", e);
                        vec![]
                    }
                },
                _ => vec![],
            }
        },
    })
}

fn row_to_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<GraphEdge> {
    let metadata_json: String = row.get(5)?;
    let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();

    Ok(GraphEdge {
        edge_id: row.get(0)?,
        source_id: row.get(1)?,
        target_id: row.get(2)?,
        relation_type: row.get(3)?,
        weight: row.get(4)?,
        metadata,
        created_at: row.get(6)?,
    })
}

fn row_to_reasoning_chain(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReasoningChain> {
    let steps_json: String = row.get(2)?;
    let steps: Vec<ReasoningStep> = serde_json::from_str(&steps_json).unwrap_or_default();

    let consulted_json: String = row.get(6)?;
    let consulted_records: Vec<String> = serde_json::from_str(&consulted_json).unwrap_or_default();

    let tags_json: String = row.get(7)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(ReasoningChain {
        chain_id: row.get(0)?,
        goal: row.get(1)?,
        steps,
        final_conclusion: row.get(3)?,
        overall_confidence: row.get(4)?,
        success: row.get::<_, i32>(5)? != 0,
        consulted_records,
        tags,
        created_at: row.get(8)?,
        duration_ms: row.get::<_, i64>(9)? as u64,
    })
}


fn row_to_reflection(row: &rusqlite::Row<'_>) -> rusqlite::Result<Reflection> {
    let planned_json: String = row.get(4)?;
    let planned_actions: Vec<String> = serde_json::from_str(&planned_json).unwrap_or_default();
    let tags_json: String = row.get(7)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(Reflection {
        reflection_id: row.get(0)?,
        topic: row.get(1)?,
        monologue: row.get(2)?,
        conclusion: row.get(3)?,
        planned_actions,
        outcome: row.get(5)?,
        confidence: row.get(6)?,
        tags,
        created_at: row.get(8)?,
    })
}

fn row_to_assessment(row: &rusqlite::Row<'_>) -> rusqlite::Result<SelfAssessment> {
    let issues_json: String = row.get(6)?;
    let issues_detected: Vec<String> = serde_json::from_str(&issues_json).unwrap_or_default();
    let recs_json: String = row.get(7)?;
    let recommendations: Vec<String> = serde_json::from_str(&recs_json).unwrap_or_default();

    Ok(SelfAssessment {
        assessment_id: row.get(0)?,
        memory_quality_score: row.get(1)?,
        coherence_score: row.get(2)?,
        staleness_score: row.get(3)?,
        diversity_score: row.get(4)?,
        overall_health: row.get(5)?,
        issues_detected,
        recommendations,
        created_at: row.get(8)?,
    })
}

fn row_to_temporal_fact(row: &rusqlite::Row<'_>) -> rusqlite::Result<TemporalFact> {
    let metadata_json: String = row.get(13)?;
    let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();

    Ok(TemporalFact {
        fact_id: row.get(0)?,
        content: row.get(1)?,
        content_type: row.get(2)?,
        valid_from: row.get(3)?,
        valid_to: row.get(4)?,
        sys_start: row.get(5)?,
        sys_end: row.get(6)?,
        version: row.get::<_, i64>(7)? as u32,
        previous_version_id: row.get(8)?,
        decay_score: row.get(9)?,
        recall_count: row.get::<_, i64>(10)? as u64,
        last_recalled: row.get(11)?,
        importance: row.get(12)?,
        metadata,
    })
}

fn row_to_context_block(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextBlock> {
    let metadata_json: String = row.get(8)?;
    let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();

    Ok(ContextBlock {
        block_id: row.get(0)?,
        label: row.get(1)?,
        content: row.get(2)?,
        pinned: row.get::<_, i32>(3)? != 0,
        priority: row.get(4)?,
        max_tokens: row.get::<_, i64>(5)? as usize,
        current_tokens: row.get::<_, i64>(6)? as usize,
        last_updated: row.get(7)?,
        metadata,
    })
}

fn row_to_context_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextSummary> {
    let source_json: String = row.get(3)?;
    let source_block_ids: Vec<String> = serde_json::from_str(&source_json).unwrap_or_default();

    Ok(ContextSummary {
        summary_id: row.get(0)?,
        topic: row.get(1)?,
        summary: row.get(2)?,
        source_block_ids,
        created_at: row.get(4)?,
    })
}

fn row_to_namespace(row: &rusqlite::Row<'_>) -> rusqlite::Result<Namespace> {
    let parents_json: String = row.get(4)?;
    let read_parents: Vec<String> = serde_json::from_str(&parents_json).unwrap_or_default();
    let children_json: String = row.get(5)?;
    let write_children: Vec<String> = serde_json::from_str(&children_json).unwrap_or_default();

    Ok(Namespace {
        namespace_id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        owner: row.get(3)?,
        read_parents,
        write_children,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_insert_and_get() {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        let record =
            MemoryRecord::new("tier-test-1".into(), "Tiered content".into(), "test".into());
        store
            .insert_into_tier(&record, MemoryTier::Working, 0.8, Some(3600), None)
            .unwrap();
        let tiered = store
            .get_tiered("tier-test-1")
            .unwrap()
            .expect("Should exist");
        assert_eq!(tiered.tier, MemoryTier::Working);
        assert!((tiered.importance - 0.8).abs() < 0.001);
        assert_eq!(tiered.ttl_seconds, Some(3600));
    }

    #[test]
    fn test_list_by_tier() {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        for i in 0..3 {
            let r = MemoryRecord::new(
                format!("e{}", i),
                format!("Episodic {}", i),
                "tier_test".into(),
            );
            store
                .insert_into_tier(&r, MemoryTier::Episodic, 0.5, None, None)
                .unwrap();
        }
        for i in 0..2 {
            let r = MemoryRecord::new(
                format!("s{}", i),
                format!("Semantic {}", i),
                "tier_test".into(),
            );
            store
                .insert_into_tier(&r, MemoryTier::Semantic, 0.9, None, None)
                .unwrap();
        }
        let episodic = store.list_by_tier(MemoryTier::Episodic, 10, 0).unwrap();
        assert_eq!(episodic.len(), 3);
        let semantic = store.list_by_tier(MemoryTier::Semantic, 10, 0).unwrap();
        assert_eq!(semantic.len(), 2);
    }

    #[test]
    fn test_graph_edges() {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        let r1 = MemoryRecord::new("g1".into(), "Node A".into(), "graph".into());
        let r2 = MemoryRecord::new("g2".into(), "Node B".into(), "graph".into());
        store.insert(&r1).unwrap();
        store.insert(&r2).unwrap();
        store.add_edge("g1", "g2", "related_to", 0.9).unwrap();
        let edges = store.get_edges("g1").unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relation_type, "related_to");
        let related = store.get_related_records("g1", None, 1).unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].0.id, "g2");
    }

    #[test]
    fn test_reasoning_chain() {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        let chain = ReasoningChain {
            chain_id: "chain-1".into(),
            goal: "Analyze market trend".into(),
            steps: vec![ReasoningStep {
                step_index: 0,
                premise: "Price increased 10%".into(),
                inference: "Check volume".into(),
                conclusion: "Volume confirms trend".into(),
                confidence: 0.85,
                tool_used: Some("volume_analyzer".into()),
                success: true,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }],
            final_conclusion: Some("Bullish trend confirmed".into()),
            overall_confidence: 0.85,
            success: true,
            consulted_records: vec!["r1".into(), "r2".into()],
            tags: vec!["market".into(), "analysis".into()],
            created_at: chrono::Utc::now().to_rfc3339(),
            duration_ms: 1500,
        };
        store.store_reasoning_chain(&chain).unwrap();
        let retrieved = store
            .get_reasoning_chain("chain-1")
            .unwrap()
            .expect("Should exist");
        assert_eq!(retrieved.goal, "Analyze market trend");
        assert_eq!(retrieved.steps.len(), 1);
        assert_eq!(retrieved.tags.len(), 2);
    }

    #[test]
    fn test_eviction() {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        for i in 0..5 {
            let r = MemoryRecord::new(
                format!("evict{}", i),
                format!("Low importance {}", i),
                "evict".into(),
            );
            store
                .insert_into_tier(&r, MemoryTier::Episodic, 0.1 * (i as f64), None, None)
                .unwrap();
        }
        let evicted = store.evict_from_tier(MemoryTier::Episodic, 3).unwrap();
        assert_eq!(evicted, 2);
        let remaining = store.list_by_tier(MemoryTier::Episodic, 10, 0).unwrap();
        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn test_stats_with_tiers() {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        let r = MemoryRecord::new("s1".into(), "Stats test".into(), "stats_test".into());
        store
            .insert_into_tier(&r, MemoryTier::Working, 0.5, Some(60), None)
            .unwrap();
        let stats = store.stats().unwrap();
        assert!(stats.total_records >= 1);
        assert!(stats.tier_breakdown.contains_key("working"));
    }

    #[test]
    fn test_full_text_search() {
        let config = StorageConfig::default();
        let store = MemoryStore::open(&config).unwrap();
        store
            .insert(&MemoryRecord::new(
                "fts1".into(),
                "Bitcoin hits all time high".into(),
                "news".into(),
            ))
            .unwrap();
        store
            .insert(&MemoryRecord::new(
                "fts2".into(),
                "Ethereum merge completed".into(),
                "news".into(),
            ))
            .unwrap();
        let results = store.search_fts("bitcoin", 10).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|(r, _)| r.id == "fts1"));
        let tier_results = store
            .search_fts_in_tier("bitcoin", MemoryTier::Episodic, 10)
            .unwrap();
        assert!(!tier_results.is_empty());
    }
}
