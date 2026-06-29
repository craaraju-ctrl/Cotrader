use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use dashmap::DashMap;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

use tresdo_exchange::api::{create_router, AppState};
use tresdo_exchange::auth::{self, ApiKeyPair};
use tresdo_exchange::engine::ExchangeEngine;
use tresdo_exchange::memory::MemoryAgentClient;
use tresdo_exchange::orchestra::{AgentConfig, Orchestra};
use tresdo_exchange::rat::stream;
use tresdo_exchange::rat::ConnectionMode;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tracing::info!("Starting Tredo Exchange v{}", env!("CARGO_PKG_VERSION"));

    // ── Initialise Engine ───────────────────────────────────
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/tredo_exchange".to_string());

    let engine = match ExchangeEngine::new_with_persistence(&database_url).await {
        Ok(e) => {
            tracing::info!("WAL persistence enabled");
            e
        }
        Err(e) => {
            tracing::warn!("Database unavailable ({}), running in-memory only", e);
            ExchangeEngine::new()
        }
    };

    // Seed demo balances (including orchestra agent user)
    {
        let risk = engine.risk_engine();
        risk.deposit("alice", "USD", 1_000_000.0);
        risk.deposit("alice", "BTC", 100.0);
        risk.deposit("bob", "USD", 500_000.0);
        risk.deposit("bob", "BTC", 50.0);
        risk.deposit("charlie", "USD", 250_000.0);
        risk.deposit("charlie", "ETH", 500.0);
        // Orchestra autonomous agent
        risk.deposit("orchestra", "USD", 100_000.0);
        risk.deposit("orchestra", "BTC", 10.0);
        tracing::info!("Demo balances seeded (including 'orchestra' agent user)");
    }

    // ── Broadcast Channels ──────────────────────────────────
    let ws_tx = tresdo_exchange::api::ws::create_broadcast_channel();
    let rat_tx = stream::create_rat_channel();

    let api_keys: Arc<DashMap<String, ApiKeyPair>> = Arc::new(DashMap::new());

    // Generate default API keys for demo users
    for user in &["alice", "bob", "charlie"] {
        let pair = auth::generate_api_key(user);
        api_keys.insert(pair.api_key.clone(), pair);
    }
    tracing::info!("Demo API keys generated for alice, bob, charlie");

    // Spawn background tasks
    engine.run_background_tasks();

    // ── Memory Agent ────────────────────────────────────────
    let memory_url = env::var("MEMORY_AGENT_URL").unwrap_or_else(|_| "http://localhost:9090".to_string());
    let memory_client: Option<Arc<MemoryAgentClient>> = match MemoryAgentClient::auto_detect(&memory_url, "tredo-orchestra-01").await {
        mc if mc.connection_mode() != ConnectionMode::Unknown => {
            tracing::info!(
                "Memory agent connected via {} at {}",
                mc.connection_mode(),
                memory_url
            );
            let mc_arc = Arc::new(mc);
            // Spawn health checks in background
            let mc_health = mc_arc.clone();
            tokio::spawn(async move {
                mc_health.run_health_checks().await;
            });
            Some(mc_arc)
        }
        _ => {
            tracing::warn!(
                "Memory agent unreachable at {} — running without cross-reference",
                memory_url
            );
            None
        }
    };

    // ── Multi-Agent Orchestra ──────────────────────────────
    let orchestra_config = AgentConfig {
        agent_id: env::var("ORCHESTRA_AGENT_ID").unwrap_or_else(|_| "tredo-orchestra-01".into()),
        enabled: env::var("ORCHESTRA_ENABLED")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(true),
        symbols: vec!["BTC/USD".into(), "ETH/USD".into()],
        max_position_size: env::var("ORCHESTRA_MAX_POSITION")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.1),
        min_confidence: env::var("ORCHESTRA_MIN_CONFIDENCE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.65),
        ..Default::default()
    };

    let orchestra = Orchestra::new(
        orchestra_config,
        engine.clone(),
        rat_tx.clone(),
        memory_client.clone(),
    );

    let orchestra_handle = orchestra.spawn();
    tracing::info!("Multi-Agent Orchestra spawned");

    // ── App State ───────────────────────────────────────────
    let state = AppState {
        engine,
        ws_tx,
        rat_tx,
        api_keys,
    };

    // ── CORS ────────────────────────────────────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = create_router(state).layer(cors);

    // ── Listen ──────────────────────────────────────────────
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Tredo Exchange listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    let shutdown_signal = async {
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::select! {
            _ = ctrl_c => tracing::info!("Received SIGINT, shutting down"),
        }
    };

    tracing::info!("Server started. Press Ctrl+C to stop.");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .unwrap();

    // Graceful shutdown: abort orchestra task
    orchestra_handle.abort();
    tracing::info!("Server shut down gracefully");
}
