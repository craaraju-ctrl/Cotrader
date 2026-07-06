//! Demo: Run the full 8-agent pipeline with ML + neurosymbolic verification.
//!
//! Usage: `cargo run --bin rat-cli demo BTC 58500.0`

use super::RatAgents;
use crate::state::SharedState;

pub async fn run_demo(state: SharedState, symbol: &str, price: f64) {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  RAT AGENT — 8-Agent Pipeline with ML + Neurosymbolic     ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let agents = RatAgents::new(state.clone());

    // Print the agent tree
    RatAgents::print_tree();
    println!();

    // Run the full pipeline
    println!("▶ Running pipeline for {} @ {:.2}...", symbol, price);
    println!();

    let chains = agents.run_pipeline(symbol, price).await;

    // Summary
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  PIPELINE SUMMARY                                          ║");
    println!("╠══════════════════════════════════════════════════════════════╣");

    for chain in &chains {
        println!("║  {:<15} {:<30} conf={:.0}% ║",
            chain.agent, &chain.conclusion[..chain.conclusion.len().min(30)],
            chain.confidence * 100.0);
    }

    println!("╚══════════════════════════════════════════════════════════════╝");
}
