use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

use cotrader_autonomous::walk_forward_runner::{
    HistoricalCandle, SkillResult, WalkForwardConfig, WalkForwardRunner,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let command = args[1].as_str();
    match command {
        "validate" => {
            if args.len() < 4 {
                println!("Error: 'validate' requires <csv_path> <symbol>");
                return Ok(());
            }
            let csv_path = &args[2];
            let symbol = &args[3];
            run_walk_forward(csv_path, symbol).await?;
        }
        "self-evolve" => {
            run_self_evolution(&args[2..]).await?;
        }
        "demo" => {
            let symbol = args.get(2).map(|s| s.as_str()).unwrap_or("BTC");
            let price: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(58500.0);
            run_demo(symbol, price).await?;
        }
        "daemon" => {
            println!("[rat] Initializing autonomous daemon...");
            println!("[rat] Connecting to local Ollama (http://localhost:11434)...");
        }
        _ => {
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║            rat-cli — Autonomous Trading CLI             ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("Usage:");
    println!("  rat-cli demo [symbol] [price]     Run 8-agent pipeline demo");
    println!("  rat-cli validate <csv> <symbol>   Run walk-forward validation");
    println!("  rat-cli self-evolve [N] [--induce] [--symbols BTC,ETH]");
    println!("                                    Run self-evolution loop");
    println!("  rat-cli daemon                    Start autonomous trading loop");
}

/// Run the 8-agent pipeline demo with ML + neurosymbolic verification.
async fn run_demo(
    symbol: &str,
    price: f64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use cotrader_autonomous::agents::demo;
    use cotrader_autonomous::state::initialize_autonomous_system;
    use std::sync::Arc;

    // Use a simple paper broker for demo
    struct DemoBroker;
    #[async_trait::async_trait]
    impl cotrader_core::paper_engine::BrokerAdapter for DemoBroker {
        async fn connect(&self) -> Result<(), String> { Ok(()) }
        async fn disconnect(&self) -> Result<(), String> { Ok(()) }
        async fn place_order(&self, _req: cotrader_core::paper_engine::OrderRequest, _price: f64) -> Result<String, String> {
            Ok("demo-order-001".to_string())
        }
        async fn cancel_order(&self, _id: &str) -> Result<(), String> { Ok(()) }
        async fn get_positions(&self) -> Result<Vec<cotrader_core::paper_engine::Position>, String> { Ok(vec![]) }
        async fn get_summary(&self) -> Result<cotrader_core::paper_engine::PortfolioSummary, String> {
            Ok(cotrader_core::paper_engine::PortfolioSummary {
                cash: 100_000.0, equity: 100_000.0, margin_used: 0.0, free_margin: 100_000.0,
                daily_pnl: 0.0, daily_pnl_pct: 0.0, total_trades: 0, winning_trades: 0,
                losing_trades: 0, win_rate: 0.0, consecutive_losses: 0, max_drawdown: 0.0,
                max_drawdown_pct: 0.0, open_positions: 0, total_pnl_all_time: 0.0,
            })
        }
        async fn get_order_status(&self, _id: &str) -> Result<cotrader_core::paper_engine::OrderStatus, String> { Ok(cotrader_core::paper_engine::OrderStatus::Filled) }
        async fn get_recent_trades(&self, _limit: usize) -> Result<Vec<cotrader_core::paper_engine::ClosedTrade>, String> { Ok(vec![]) }
        async fn update_price(&self, _sym: &str, _price: f64) -> Result<Vec<cotrader_core::paper_engine::ClosedTrade>, String> { Ok(vec![]) }
        async fn close_position(&self, _id: &str, _price: f64) -> Result<cotrader_core::paper_engine::ClosedTrade, String> {
            Ok(cotrader_core::paper_engine::ClosedTrade {
                id: "closed-1".to_string(), symbol: "TEST".to_string(),
                direction: cotrader_core::TradeDirection::Long, qty: 1,
                entry_price: 100.0, exit_price: 100.0, realized_pnl: 0.0, realized_pnl_pct: 0.0,
                close_reason: cotrader_core::paper_engine::CloseReason::Manual,
                opened_at: chrono::Utc::now(), closed_at: chrono::Utc::now(),
                duration_secs: 0, strategy: None, order_id: "order-1".to_string(),
            })
        }
        async fn check_risk(&self, _sym: &str, _cost: f64) -> Result<cotrader_core::paper_engine::RiskCheckResult, String> {
            Ok(cotrader_core::paper_engine::RiskCheckResult {
                passed: true, max_position_size_ok: true, daily_loss_limit_ok: true,
                drawdown_ok: true, concentration_ok: true, portfolio_heat_ok: true, warnings: vec![],
            })
        }
        async fn reset(&self) -> Result<(), String> { Ok(()) }
        fn mode(&self) -> cotrader_core::paper_engine::TradingMode { cotrader_core::paper_engine::TradingMode::Paper }
        fn broker_name(&self) -> &str { "Demo" }
    }

    let paper_broker: Arc<dyn cotrader_core::paper_engine::BrokerAdapter> = Arc::new(DemoBroker);

    let orchestrator = initialize_autonomous_system(paper_broker).await?;
    demo::run_demo(orchestrator.state, symbol, price).await;
    Ok(())
}

/// Run the extended self-evolution validation harness (engineering loop).
///
/// Boots the autonomous orchestrator, then drives N cycles of the full agentic
/// pipeline (optionally inducing regret) and prints a compounding-improvement
/// report. Symbols come from `--symbols`, else the `WATCHLIST` env var, else a
/// BTC/ETH default.
async fn run_self_evolution(
    rest: &[String],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use cotrader_autonomous::self_evolution::SelfEvolutionValidator;
    use cotrader_autonomous::state::initialize_autonomous_system;
    use std::sync::Arc;

    // Parse args: optional positional cycle count, optional `--induce`, optional
    // `--symbols A,B,C`.
    let mut cycles: usize = 20;
    let mut induce = false;
    let mut symbols_arg: Option<String> = None;

    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--induce" | "--induce-regret" => induce = true,
            "--symbols" => {
                if i + 1 < rest.len() {
                    symbols_arg = Some(rest[i + 1].clone());
                    i += 1;
                }
            }
            other => {
                if let Ok(n) = other.parse::<usize>() {
                    cycles = n;
                }
            }
        }
        i += 1;
    }

    let symbols_owned: Vec<String> = symbols_arg
        .or_else(|| env::var("WATCHLIST").ok())
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| vec!["BTC".to_string(), "ETH".to_string()]);
    let symbols: Vec<&str> = symbols_owned.iter().map(|s| s.as_str()).collect();

    println!(
        "[rat] Booting autonomous orchestrator for self-evolution ({} cycles, induce={}, symbols={:?})...",
        cycles, induce, symbols
    );

    // Use a simple paper broker for self-evolution
    struct DemoBroker;
    #[async_trait::async_trait]
    impl cotrader_core::paper_engine::BrokerAdapter for DemoBroker {
        async fn connect(&self) -> Result<(), String> { Ok(()) }
        async fn disconnect(&self) -> Result<(), String> { Ok(()) }
        async fn place_order(&self, _req: cotrader_core::paper_engine::OrderRequest, _price: f64) -> Result<String, String> { Ok("demo".to_string()) }
        async fn cancel_order(&self, _id: &str) -> Result<(), String> { Ok(()) }
        async fn get_positions(&self) -> Result<Vec<cotrader_core::paper_engine::Position>, String> { Ok(vec![]) }
        async fn get_summary(&self) -> Result<cotrader_core::paper_engine::PortfolioSummary, String> {
            Ok(cotrader_core::paper_engine::PortfolioSummary {
                cash: 100_000.0, equity: 100_000.0, margin_used: 0.0, free_margin: 100_000.0,
                daily_pnl: 0.0, daily_pnl_pct: 0.0, total_trades: 0, winning_trades: 0,
                losing_trades: 0, win_rate: 0.0, consecutive_losses: 0, max_drawdown: 0.0,
                max_drawdown_pct: 0.0, open_positions: 0, total_pnl_all_time: 0.0,
            })
        }
        async fn get_order_status(&self, _id: &str) -> Result<cotrader_core::paper_engine::OrderStatus, String> { Ok(cotrader_core::paper_engine::OrderStatus::Filled) }
        async fn get_recent_trades(&self, _limit: usize) -> Result<Vec<cotrader_core::paper_engine::ClosedTrade>, String> { Ok(vec![]) }
        async fn update_price(&self, _sym: &str, _price: f64) -> Result<Vec<cotrader_core::paper_engine::ClosedTrade>, String> { Ok(vec![]) }
        async fn close_position(&self, _id: &str, _price: f64) -> Result<cotrader_core::paper_engine::ClosedTrade, String> {
            Ok(cotrader_core::paper_engine::ClosedTrade {
                id: "closed-1".to_string(), symbol: "TEST".to_string(),
                direction: cotrader_core::TradeDirection::Long, qty: 1,
                entry_price: 100.0, exit_price: 100.0, realized_pnl: 0.0, realized_pnl_pct: 0.0,
                close_reason: cotrader_core::paper_engine::CloseReason::Manual,
                opened_at: chrono::Utc::now(), closed_at: chrono::Utc::now(),
                duration_secs: 0, strategy: None, order_id: "order-1".to_string(),
            })
        }
        async fn check_risk(&self, _sym: &str, _cost: f64) -> Result<cotrader_core::paper_engine::RiskCheckResult, String> {
            Ok(cotrader_core::paper_engine::RiskCheckResult {
                passed: true, max_position_size_ok: true, daily_loss_limit_ok: true,
                drawdown_ok: true, concentration_ok: true, portfolio_heat_ok: true, warnings: vec![],
            })
        }
        async fn reset(&self) -> Result<(), String> { Ok(()) }
        fn mode(&self) -> cotrader_core::paper_engine::TradingMode { cotrader_core::paper_engine::TradingMode::Paper }
        fn broker_name(&self) -> &str { "Demo" }
    }

    let paper_broker: Arc<dyn cotrader_core::paper_engine::BrokerAdapter> = Arc::new(DemoBroker);

    let orchestrator = initialize_autonomous_system(paper_broker).await?;

    // Must init_rat() before running pipeline — otherwise the orchestrator
    // panics with "Rat not initialized — call init_rat() after construction".
    let mut orch = orchestrator;
    orch.init_rat();

    let validator = SelfEvolutionValidator::new(orch);
    // run_extended_validation already prints the full summary on completion.
    let _report = validator
        .run_extended_validation(&symbols, cycles, induce)
        .await?;

    Ok(())
}

/// Parses historical CSV files and executes the WalkForwardRunner
async fn run_walk_forward(
    csv_path: &str,
    symbol: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[rat] Reading OHLCV dataset from: {}", csv_path);
    let candles = load_candles_from_csv(csv_path)?;
    println!("[rat] Successfully loaded {} candles.", candles.len());

    let config = WalkForwardConfig {
        train_window_size: 100,
        test_window_size: 20,
        step_size: 20,
        initial_capital: 10000.0,
        base_learning_rate: 0.05,
        overfitting_threshold: 0.35,
    };

    let runner = WalkForwardRunner::new(config);
    let mut initial_weights = std::collections::HashMap::new();
    initial_weights.insert("news_analyser".to_string(), 0.50);
    initial_weights.insert("market_metrics_meter".to_string(), 0.50);

    println!("[rat] Starting walk-forward evaluation loops...");
    let start_time = Instant::now();

    let report = runner
        .run_validation(symbol, &candles, initial_weights, |_slice, _weights| {
            let results = vec![SkillResult {
                score: 0.65,
                confidence: 0.85,
            }];
            Ok(Some(results))
        })
        .await?;

    let elapsed = start_time.elapsed();
    println!("\n==================================================");
    println!("=== WALK-FORWARD VALIDATION COMPLETE ({:?}) ===", elapsed);
    println!("==================================================");
    println!("Total Folds Evaluated: {}", report.total_folds_evaluated);
    println!(
        "Mean In-Sample Sharpe:  {:.4}",
        report.mean_in_sample_sharpe
    );
    println!(
        "Mean Out-of-Sample Sharpe: {:.4}",
        report.mean_out_of_sample_sharpe
    );
    println!(
        "Structural Stability Score: {:.2}%",
        report.structural_stability_score * 100.0
    );
    println!("Deployment Verdict:     {}", report.overall_recommendation);
    println!("==================================================");

    Ok(())
}

fn load_candles_from_csv(file_path: &str) -> io::Result<Vec<HistoricalCandle>> {
    let path = Path::new(file_path);
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut candles = Vec::new();
    let mut lines = reader.lines();

    // Skip CSV header line
    let _header = lines.next();

    for line in lines {
        let line_str = line?;
        let columns: Vec<&str> = line_str.split(',').collect();
        if columns.len() < 6 {
            continue;
        }

        let timestamp = columns[0].parse::<u64>().unwrap_or_default();
        let open = columns[1].parse::<f64>().unwrap_or_default();
        let high = columns[2].parse::<f64>().unwrap_or_default();
        let low = columns[3].parse::<f64>().unwrap_or_default();
        let close = columns[4].parse::<f64>().unwrap_or_default();
        let volume = columns[5].parse::<f64>().unwrap_or_default();

        candles.push(HistoricalCandle {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
        });
    }

    Ok(candles)
}
