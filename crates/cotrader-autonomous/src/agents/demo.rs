//! Demo: Run the full 8-agent pipeline with ML + neurosymbolic verification.
//!
//! Usage: `cargo run --bin rat-cli demo BTC 58500.0`

use super::RatAgents;
use crate::episode_store::EpisodeStore;
use crate::state::SharedState;
use std::sync::Arc;

pub async fn run_demo(state: SharedState, symbol: &str, price: f64) {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  RAT AGENT — 8-Agent Pipeline with ML + Neurosymbolic     ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Build ML engine and episode store from state
    let ml_engine = state.ml_engine.clone();
    let episode_store = state.agent_memory.episode_store.clone();

    let cb = std::sync::Arc::new(crate::resilience::CircuitBreakerHierarchy::new());
    let agents = RatAgents::new(ml_engine, episode_store, cb);

    // Print the agent tree
    RatAgents::print_tree();
    println!();

    // Run the full pipeline
    println!("▶ Running pipeline for {} @ {:.2}...", symbol, price);
    println!();

    // Build CacheFrame from SharedState
    let frame = state.build_cache_frame(symbol, price, 1).await;

    let (chains, _intent, _events) = agents.run_pipeline(frame).await;

    // Summary
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  PIPELINE SUMMARY                                          ║");
    println!("╠══════════════════════════════════════════════════════════════╣");

    for chain in &chains {
        let conclusion_preview = if chain.conclusion.len() > 30 {
            &chain.conclusion[..30]
        } else {
            &chain.conclusion
        };
        println!("║  {:<15} {:<30} conf={:.0}% ║",
            chain.agent, conclusion_preview,
            chain.confidence * 100.0);
    }

    println!("╚══════════════════════════════════════════════════════════════╝");
}
