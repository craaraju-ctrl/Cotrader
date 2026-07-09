// Embedded Schema Migrations
// Versioned migration system for safe schema upgrades.

use rusqlite::{params, Connection};

/// A single schema migration.
pub struct Migration {
    pub version: u32,
    pub description: &'static str,
    pub up: fn(&Connection) -> rusqlite::Result<()>,
}

/// Get all migrations in order.
pub fn all_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "Initial schema: records, graph_edges, reasoning_chains, expert_opinions, evolution_events, reflections, self_assessments, temporal_facts, context_blocks, context_summaries, tier_config, namespaces",
            up: migration_001_initial,
        },
        Migration {
            version: 2,
            description: "Add tags_json column to records",
            up: migration_002_add_tags,
        },
        Migration {
            version: 3,
            description: "Add namespace_id column to records",
            up: migration_003_add_namespace,
        },
        Migration {
            version: 4,
            description: "Add composite indexes for high-frequency queries on records",
            up: migration_004_add_indexes,
        },
        Migration {
            version: 5,
            description: "Add strategy_ratings table for star-rating confidence persistence",
            up: migration_005_strategy_ratings,
        },
    ]
}

/// Migration 001: Initial schema
fn migration_001_initial(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS records (
            id            TEXT PRIMARY KEY,
            content       TEXT NOT NULL,
            content_type  TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            embedding     BLOB,
            timestamp     TEXT NOT NULL,
            tier          TEXT NOT NULL DEFAULT 'working',
            importance    REAL NOT NULL DEFAULT 0.5,
            access_count  INTEGER NOT NULL DEFAULT 0,
            last_accessed TEXT,
            ttl_seconds   INTEGER,
            parent_id     TEXT,
            valid_from    TEXT,
            valid_to      TEXT,
            sys_start     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            sys_end       TEXT,
            tags_json     TEXT NOT NULL DEFAULT '[]',
            namespace_id  TEXT NOT NULL DEFAULT 'default'
        );

        CREATE INDEX IF NOT EXISTS idx_records_tier ON records(tier);
        CREATE INDEX IF NOT EXISTS idx_records_type ON records(content_type);
        CREATE INDEX IF NOT EXISTS idx_records_ts ON records(timestamp);
        CREATE INDEX IF NOT EXISTS idx_records_parent ON records(parent_id);
        CREATE INDEX IF NOT EXISTS idx_records_importance ON records(importance);
        CREATE INDEX IF NOT EXISTS idx_records_namespace ON records(namespace_id);

        CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
            id UNINDEXED, content, content_type UNINDEXED, metadata_json UNINDEXED
        );

        CREATE TABLE IF NOT EXISTS tier_config (
            tier TEXT PRIMARY KEY,
            max_records INTEGER NOT NULL DEFAULT 1000,
            default_ttl_secs INTEGER,
            promotion_threshold REAL NOT NULL DEFAULT 0.7,
            demotion_threshold REAL NOT NULL DEFAULT 0.2,
            auto_promote INTEGER NOT NULL DEFAULT 1
        );

        INSERT OR IGNORE INTO tier_config VALUES ('working', 100, 3600, 0.5, 0.1, 1);
        INSERT OR IGNORE INTO tier_config VALUES ('episodic', 10000, 2592000, 0.7, 0.2, 1);
        INSERT OR IGNORE INTO tier_config VALUES ('semantic', 100000, NULL, 0.85, 0.15, 0);
        INSERT OR IGNORE INTO tier_config VALUES ('procedural', 10000, NULL, 0.95, 0.1, 0);

        CREATE TABLE IF NOT EXISTS graph_edges (
            edge_id TEXT PRIMARY KEY,
            source_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            weight REAL NOT NULL DEFAULT 1.0,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            FOREIGN KEY (source_id) REFERENCES records(id) ON DELETE CASCADE,
            FOREIGN KEY (target_id) REFERENCES records(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_edges_source ON graph_edges(source_id);
        CREATE INDEX IF NOT EXISTS idx_edges_target ON graph_edges(target_id);
        CREATE INDEX IF NOT EXISTS idx_edges_relation ON graph_edges(relation_type);

        CREATE TABLE IF NOT EXISTS reasoning_chains (
            chain_id TEXT PRIMARY KEY,
            goal TEXT NOT NULL,
            steps_json TEXT NOT NULL DEFAULT '[]',
            final_conclusion TEXT,
            overall_confidence REAL NOT NULL DEFAULT 0.0,
            success INTEGER NOT NULL DEFAULT 0,
            consulted_records TEXT NOT NULL DEFAULT '[]',
            tags_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            duration_ms INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_chains_goal ON reasoning_chains(goal);

        CREATE TABLE IF NOT EXISTS expert_opinions (
            opinion_id TEXT PRIMARY KEY,
            expert_type TEXT NOT NULL,
            target_record_id TEXT,
            recommendation TEXT NOT NULL,
            reasoning TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0.0,
            action_taken TEXT,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            FOREIGN KEY (target_record_id) REFERENCES records(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_opinions_expert ON expert_opinions(expert_type);
        CREATE INDEX IF NOT EXISTS idx_opinions_target ON expert_opinions(target_record_id);

        CREATE TABLE IF NOT EXISTS evolution_events (
            event_id TEXT PRIMARY KEY,
            event_type TEXT NOT NULL,
            description TEXT NOT NULL,
            previous_value TEXT,
            new_value TEXT,
            confidence REAL NOT NULL DEFAULT 0.0,
            timestamp TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        CREATE INDEX IF NOT EXISTS idx_evolution_type ON evolution_events(event_type);

        CREATE TABLE IF NOT EXISTS reflections (
            reflection_id TEXT PRIMARY KEY,
            topic TEXT NOT NULL,
            monologue TEXT NOT NULL,
            conclusion TEXT NOT NULL,
            planned_actions TEXT NOT NULL DEFAULT '[]',
            outcome TEXT,
            confidence REAL NOT NULL DEFAULT 0.0,
            tags_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        CREATE INDEX IF NOT EXISTS idx_reflections_topic ON reflections(topic);
        CREATE INDEX IF NOT EXISTS idx_reflections_created ON reflections(created_at);

        CREATE TABLE IF NOT EXISTS self_assessments (
            assessment_id TEXT PRIMARY KEY,
            memory_quality_score REAL NOT NULL DEFAULT 0.0,
            coherence_score REAL NOT NULL DEFAULT 0.0,
            staleness_score REAL NOT NULL DEFAULT 0.0,
            diversity_score REAL NOT NULL DEFAULT 0.0,
            overall_health REAL NOT NULL DEFAULT 0.0,
            issues_detected TEXT NOT NULL DEFAULT '[]',
            recommendations TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        CREATE INDEX IF NOT EXISTS idx_assessments_created ON self_assessments(created_at);

        CREATE TABLE IF NOT EXISTS temporal_facts (
            fact_id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            content_type TEXT NOT NULL,
            valid_from TEXT NOT NULL,
            valid_to TEXT,
            sys_start TEXT NOT NULL,
            sys_end TEXT,
            version INTEGER NOT NULL DEFAULT 1,
            previous_version_id TEXT,
            decay_score REAL NOT NULL DEFAULT 1.0,
            recall_count INTEGER NOT NULL DEFAULT 0,
            last_recalled TEXT,
            importance REAL NOT NULL DEFAULT 0.5,
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_temporal_type ON temporal_facts(content_type);
        CREATE INDEX IF NOT EXISTS idx_temporal_valid ON temporal_facts(valid_from);
        CREATE INDEX IF NOT EXISTS idx_temporal_version ON temporal_facts(previous_version_id);

        CREATE TABLE IF NOT EXISTS context_blocks (
            block_id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            content TEXT NOT NULL,
            pinned INTEGER NOT NULL DEFAULT 0,
            priority INTEGER NOT NULL DEFAULT 0,
            max_tokens INTEGER NOT NULL DEFAULT 1024,
            current_tokens INTEGER NOT NULL DEFAULT 0,
            last_updated TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_blocks_label ON context_blocks(label);

        CREATE TABLE IF NOT EXISTS context_summaries (
            summary_id TEXT PRIMARY KEY,
            topic TEXT NOT NULL,
            summary TEXT NOT NULL,
            source_block_ids TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        CREATE INDEX IF NOT EXISTS idx_summaries_topic ON context_summaries(topic);

        CREATE TABLE IF NOT EXISTS namespaces (
            namespace_id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT NOT NULL DEFAULT '',
            owner TEXT NOT NULL,
            read_parents TEXT NOT NULL DEFAULT '[]',
            write_children TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX IF NOT EXISTS idx_namespaces_name ON namespaces(name);
        CREATE INDEX IF NOT EXISTS idx_namespaces_owner ON namespaces(owner);
        ",
    )?;
    Ok(())
}

/// Migration 002: Add tags_json column
fn migration_002_add_tags(conn: &Connection) -> rusqlite::Result<()> {
    // Check if column already exists
    let has_tags: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('records') WHERE name = 'tags_json'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
        > 0;

    if !has_tags {
        conn.execute_batch("ALTER TABLE records ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';")?;
    }
    Ok(())
}

/// Migration 003: Add namespace_id column
fn migration_003_add_namespace(conn: &Connection) -> rusqlite::Result<()> {
    let has_ns: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('records') WHERE name = 'namespace_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
        > 0;

    if !has_ns {
        conn.execute_batch("ALTER TABLE records ADD COLUMN namespace_id TEXT NOT NULL DEFAULT 'default';")?;
    }
    Ok(())
}

/// Migration 004: Add composite indexes for high-frequency queries.
/// Applies indexes on [timestamp, importance, namespace_id, content_type]
/// to guarantee sub-millisecond table scanning for the active memory segments.
fn migration_004_add_indexes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        -- Composite index: timestamp + importance (for time-sorted eviction candidates)
        CREATE INDEX IF NOT EXISTS idx_records_ts_importance
            ON records(timestamp, importance);

        -- Composite index: namespace_id + importance (for per-namespace pruning)
        CREATE INDEX IF NOT EXISTS idx_records_ns_importance
            ON records(namespace_id, importance);

        -- Composite index: namespace_id + timestamp (for per-namespace time queries)
        CREATE INDEX IF NOT EXISTS idx_records_ns_ts
            ON records(namespace_id, timestamp);

        -- Composite index: content_type + importance (for dedup within type)
        CREATE INDEX IF NOT EXISTS idx_records_type_importance
            ON records(content_type, importance);

        -- Composite index: tier + importance (for tier-aware eviction)
        CREATE INDEX IF NOT EXISTS idx_records_tier_importance
            ON records(tier, importance);

        -- Index on expert_opinions for time-based queries
        CREATE INDEX IF NOT EXISTS idx_opinions_created
            ON expert_opinions(created_at);

        -- Index on temporal_facts for decay-based queries
        CREATE INDEX IF NOT EXISTS idx_temporal_importance
            ON temporal_facts(importance);
        CREATE INDEX IF NOT EXISTS idx_temporal_decay
            ON temporal_facts(decay_score);
        ",
    )?;
    Ok(())
}

/// Migration 005: Add strategy_ratings table for star-rating confidence persistence.
/// Stores stamped star ratings from FinancialRegretScorer for orchestrator retrieval.
fn migration_005_strategy_ratings(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS strategy_ratings (
            id              TEXT PRIMARY KEY,
            namespace_id    TEXT NOT NULL DEFAULT 'default',
            confidence_tier TEXT NOT NULL,  -- 'SingleStar', 'DoubleStar', 'TripleStar'
            raw_importance  REAL NOT NULL,
            blended_score   REAL NOT NULL,
            sample_size     INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        -- Clean up orphaned ratings when records are evicted/deleted
        -- Note: SQLite doesn't enforce FK across tables in all cases,
        -- but the composite index ensures fast cleanup during Phase 5 eviction.

        -- Composite index for fast per-namespace star-rating queries
        CREATE INDEX IF NOT EXISTS idx_strategy_ratings_ns_tier
            ON strategy_ratings(namespace_id, confidence_tier);

        -- Index for time-based queries
        CREATE INDEX IF NOT EXISTS idx_strategy_ratings_created
            ON strategy_ratings(created_at);
        ",
    )?;
    Ok(())
}

/// Run all pending migrations.
pub fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    // Create migration tracking table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            description TEXT,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;

    // Get current version
    let current_version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Run pending migrations
    let migrations = all_migrations();
    for migration in &migrations {
        if migration.version > current_version {
            tracing::info!(
                "Running migration v{}: {}",
                migration.version,
                migration.description
            );
            (migration.up)(conn)?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version, description) VALUES (?1, ?2)",
                params![migration.version, migration.description],
            )?;
        }
    }

    Ok(())
}
