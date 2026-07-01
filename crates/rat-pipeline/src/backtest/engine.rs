//! Backtest Engine — Main backtesting logic.

pub struct BacktestEngine {
    pub initial_capital: f64,
    pub commission: f64,
    pub slippage: f64,
}

impl BacktestEngine {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            initial_capital,
            commission: 0.001, // 0.1%
            slippage: 0.0005,  // 0.05%
        }
    }

    /// Run a backtest on historical data.
    pub async fn run(&self, data: &[Bar], strategy: &dyn Strategy) -> BacktestResult {
        let mut equity = self.initial_capital;
        let mut peak_equity = equity;
        let mut max_drawdown = 0.0;
        let mut trades = Vec::new();
        let mut equity_curve = vec![equity];

        for bar in data {
            let signal = strategy.generate_signal(bar, &equity);

            if let Some(signal) = signal {
                let entry_price = bar.close * (1.0 + self.slippage);
                let cost = entry_price * self.commission;

                match signal {
                    Signal::Buy { size } => {
                        let total_cost = entry_price * size + cost;
                        if total_cost <= equity {
                            equity -= total_cost;
                            trades.push(Trade {
                                entry: entry_price,
                                exit: 0.0,
                                size,
                                pnl: 0.0,
                                timestamp: bar.timestamp,
                            });
                        }
                    }
                    Signal::Sell { size: _ } => {
                        if let Some(trade) = trades.last_mut() {
                            trade.exit = bar.close * (1.0 - self.slippage);
                            trade.pnl = (trade.exit - trade.entry) * trade.size - cost;
                            equity += trade.entry * trade.size + trade.pnl;
                        }
                    }
                    Signal::Hold => {}
                }
            }

            // Track drawdown
            if equity > peak_equity {
                peak_equity = equity;
            }
            let dd = (peak_equity - equity) / peak_equity;
            if dd > max_drawdown {
                max_drawdown = dd;
            }

            equity_curve.push(equity);
        }

        // Calculate statistics
        let winning_trades = trades.iter().filter(|t| t.pnl > 0.0).count();
        let total_trades = trades.len();
        let win_rate = if total_trades > 0 { winning_trades as f64 / total_trades as f64 } else { 0.0 };
        let avg_win = trades.iter().filter(|t| t.pnl > 0.0).map(|t| t.pnl).sum::<f64>() / winning_trades.max(1) as f64;
        let avg_loss = trades.iter().filter(|t| t.pnl < 0.0).map(|t| t.pnl.abs()).sum::<f64>() / (total_trades - winning_trades).max(1) as f64;
        let profit_factor = if avg_loss > 0.0 { avg_win / avg_loss } else { 0.0 };

        let total_return = (equity - self.initial_capital) / self.initial_capital;

        BacktestResult {
            initial_capital: self.initial_capital,
            final_equity: equity,
            total_return,
            max_drawdown,
            win_rate,
            total_trades: total_trades as u32,
            winning_trades: winning_trades as u32,
            losing_trades: (total_trades - winning_trades) as u32,
            avg_win,
            avg_loss,
            profit_factor,
            equity_curve,
            trades,
        }
    }
}

/// Trading signal.
pub enum Signal {
    Buy { size: f64 },
    Sell { size: f64 },
    Hold,
}

/// Strategy trait.
pub trait Strategy {
    fn generate_signal(&self, bar: &Bar, equity: &f64) -> Option<Signal>;
}

/// OHLCV bar.
#[derive(Debug, Clone)]
pub struct Bar {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Trade record.
#[derive(Debug, Clone)]
pub struct Trade {
    pub entry: f64,
    pub exit: f64,
    pub size: f64,
    pub pnl: f64,
    pub timestamp: i64,
}

/// Backtest result.
#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub initial_capital: f64,
    pub final_equity: f64,
    pub total_return: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub equity_curve: Vec<f64>,
    pub trades: Vec<Trade>,
}
