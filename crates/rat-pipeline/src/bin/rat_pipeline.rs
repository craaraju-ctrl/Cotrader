//! RAT Pipeline — Main entry point for the trading system.
//!
//! Usage: rat-pipeline --symbols BTC,ETH,SOL --mode paper

use rat_pipeline::runner::pipeline::{PipelineRunner, PipelineEvent};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let symbols = parse_symbols(&args);
    let mode = parse_mode(&args);

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              RAT Trading System Pipeline                    ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    let display = if symbols.len() > 10 {
        format!("{} assets ({}...)", symbols.len(), symbols[..10.min(symbols.len())].join(", "))
    } else {
        symbols.join(", ")
    };
    println!("║  Symbols: {:<48} ║", display);
    println!("║  Mode:    {:<48} ║", mode);
    println!("║  Memory:  {:<48} ║", "http://localhost:3111");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Initialize pipeline
    let mut runner = PipelineRunner::new(symbols.clone());

    // Connect to memory service
    let memory_url = std::env::var("MEMORY_API_URL")
        .unwrap_or_else(|_| "http://localhost:3111".to_string());
    let memory_client = agentic_memory::client::MemoryClient::new(&memory_url);

    // Check memory health
    match memory_client.health().await {
        Ok(status) => println!("[Memory] Connected: {} (records: {})", status.status, status.total_records),
        Err(e) => println!("[Memory] Warning: {} (continuing without memory)", e),
    }

    // Start the pipeline
    println!("[Pipeline] Starting trading pipeline...");
    runner.start().await;

    // Subscribe to events
    let mut rx = runner.event_tx.subscribe();

    // Main loop - process events and display status
    println!("[Pipeline] Pipeline running. Press Ctrl+C to stop.\n");

    let mut trade_count: HashMap<String, u32> = HashMap::new();
    let mut start_time = std::time::Instant::now();

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(PipelineEvent::MarketData { symbol, price, .. }) => {
                        print!("\r[{:>8}] ${:.2} ", symbol, price);
                    }
                    Ok(PipelineEvent::SignalGenerated { symbol, action, confidence }) => {
                        let color = match action.as_str() {
                            "BUY" => "\x1b[32m",  // Green
                            "SELL" => "\x1b[31m",  // Red
                            _ => "\x1b[33m",        // Yellow
                        };
                        println!("{} → {} ({:.0}% confidence)\x1b[0m", color, action, confidence * 100.0);

                        // Store signal in memory
                        let content = format!("Signal: {} {} (confidence: {:.0}%)", symbol, action, confidence * 100.0);
                        let _ = memory_client.insert(
                            &content,
                            "signal",
                            "episodic",
                            confidence,
                        ).await;
                    }
                    Ok(PipelineEvent::RiskChecked { symbol, passed, reason }) => {
                        if !passed {
                            println!("  ⚠️  RISK BLOCKED: {} ({})", symbol, reason);
                        }
                    }
                    Ok(PipelineEvent::TradeExecuted { symbol, action, size, price }) => {
                        *trade_count.entry(symbol.clone()).or_insert(0) += 1;
                        println!("  ✅ EXECUTED: {} {} {:.6} @ ${:.2}", action, symbol, size, price);

                        // Store trade in memory
                        let content = format!("Trade: {} {} {:.6} @ ${:.2}", action, symbol, size, price);
                        let _ = memory_client.insert(
                            &content,
                            "trade",
                            "episodic",
                            0.8,
                        ).await;
                    }
                    Ok(PipelineEvent::TradeOutcome { symbol, pnl, lesson }) => {
                        let color = if pnl >= 0.0 { "\x1b[32m" } else { "\x1b[31m" };
                        println!("  {}P&L: ${:.2} — {}\x1b[0m", color, pnl, lesson);

                        // Store outcome in memory
                        let content = format!("Outcome: {} P&L ${:.2} — {}", symbol, pnl, lesson);
                        let _ = memory_client.insert(
                            &content,
                            "outcome",
                            "episodic",
                            if pnl > 0.0 { 0.7 } else { 0.3 },
                        ).await;
                    }
                    Ok(PipelineEvent::AgentDecision { agent, decision, .. }) => {
                        println!("  🤖 [{}] {}", agent, decision);
                    }
                    Ok(PipelineEvent::Error { source, message }) => {
                        println!("  ❌ ERROR [{}]: {}", source, message);
                    }
                    Err(_) => {
                        println!("[Pipeline] Channel closed. Shutting down.");
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n[Pipeline] Shutting down gracefully...");
                break;
            }
        }

        // Print stats every 30 seconds
        if start_time.elapsed().as_secs() % 30 == 0 {
            let elapsed = start_time.elapsed().as_secs();
            let total_trades: u32 = trade_count.values().sum();
            println!("\n--- Stats: {}s elapsed, {} trades ---", elapsed, total_trades);
            for (sym, count) in &trade_count {
                println!("  {}: {} trades", sym, count);
            }
        }
    }

    println!("[Pipeline] Stopped. Total trades: {}", trade_count.values().sum::<u32>());
    Ok(())
}

fn parse_symbols(args: &[String]) -> Vec<String> {
    for i in 1..args.len() {
        if args[i] == "--symbols" && i + 1 < args.len() {
            return args[i + 1].split(',').map(|s| s.to_uppercase()).collect();
        }
        if args[i] == "--watchlist" {
            // --watchlist: fetch from orchestrator API (optionally with custom URL)
            let url = if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                args[i + 1].clone()
            } else {
                "http://localhost:8080".to_string()
            };
            let rt = tokio::runtime::Handle::current();
            match tokio::task::block_in_place(|| {
                rt.block_on(fetch_watchlist_from_orchestrator(&url))
            }) {
                Ok(symbols) => return symbols,
                Err(e) => {
                    eprintln!("[Watchlist] ⚠ Failed to fetch from {}: {}", url, e);
                    eprintln!("[Watchlist] Falling back to default crypto watchlist");
                }
            }
            break;
        }
    }
    // Default crypto watchlist (~99 symbols from SharedState default_watchlist)
    // Only crypto symbols (fetched via Binance USDT pairs); stocks/ETFs/India excluded.
    default_crypto_watchlist()
}

/// Fetch the watchlist from the orchestrator's HTTP API at `/api/watchlist`,
/// filter to only crypto symbols, and return the list.
///
/// The orchestrator runs on port 8080 by default and serves the full watchlist
/// (including stocks, ETFs, India stocks). This function filters out non-crypto
/// symbols since the pipeline trades via Binance USDT pairs.
async fn fetch_watchlist_from_orchestrator(base_url: &str) -> anyhow::Result<Vec<String>> {
    let url = format!("{}/api/watchlist", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        anyhow::bail!("Orchestrator returned status {}", resp.status());
    }

    let all_symbols: Vec<String> = resp
        .json::<Vec<String>>()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    // Filter to crypto-only symbols (the pipeline fetches prices via Binance USDT pairs)
    let crypto: Vec<String> = all_symbols
        .into_iter()
        .filter(|s| rat_core::is_crypto_symbol(s))
        .map(|s| s.to_uppercase())
        .collect();

    if crypto.is_empty() {
        anyhow::bail!("No crypto symbols found in orchestrator watchlist");
    }

    eprintln!(
        "[Watchlist] ✅ Loaded {} crypto symbols from orchestrator ({})",
        crypto.len(),
        url
    );
    Ok(crypto)
}

/// Full crypto watchlist — matches the crypto subset of SharedState::default_watchlist().
fn default_crypto_watchlist() -> Vec<String> {
    vec![
        // ── Layer1 / Smart Contract Platforms (25) ──
        "BTC","ETH","SOL","BNB","ADA","AVAX","DOT","MATIC","NEAR","ATOM",
        "FTM","ALGO","HBAR","ICP","XTZ","EGLD","FLOW","MINA","KSM","SEI","APT","INJ","SUI","TON","TRX",
        // ── DeFi / DEX / Lending (18) ──
        "UNI","AAVE","CRV","CAKE","SUSHI","COMP","MKR","SNX","BAL","YFI",
        "LDO","RPL","FXS","CVX","GMX","GNS","JOE","VELO",
        // ── Oracles / Infrastructure (6) ──
        "LINK","GRT","BAND","API3","TRB","UMA",
        // ── Payments / Currency / Privacy (7) ──
        "XRP","LTC","XLM","DASH","ZEC","XMR","NANO",
        // ── Gaming / Metaverse (10) ──
        "AXS","SAND","MANA","GALA","ENJ","CHZ","ILV","YGG","IMX","RON",
        // ── Meme / Community (8) ──
        "DOGE","SHIB","PEPE","WIF","BONK","FLOKI","BABYDOGE","ELON",
        // ── Layer2 / Scaling (6) ──
        "ARB","OP","LRC","BOBA","METIS","CTSI",
        // ── Storage / Compute / Data (4) ──
        "FIL","AR","STORJ","AKT",
        // ── Exchange / Platform Tokens (5) ──
        "CRO","OKB","KCS","LEO","HT",
        // ── AI / Data / Emerging (10) ──
        "FET","AGIX","OCEAN","RNDR","TAO","ARKM","NMR","TRAC","ORAI","MDT",
    ].into_iter().map(String::from).collect()
}

fn parse_mode(args: &[String]) -> String {
    for i in 1..args.len() {
        if args[i] == "--mode" && i + 1 < args.len() {
            return args[i + 1].clone();
        }
    }
    "paper".to_string()
}
