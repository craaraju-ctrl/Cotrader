//! Backtest Report — Generates comprehensive backtest reports.

use super::engine::BacktestResult;

pub struct BacktestReport;

impl BacktestReport {
    pub fn generate(result: &BacktestResult) -> String {
        format!(
            "Backtest Report\n\
             ===============\n\
             Initial Capital: ${:.2}\n\
             Final Equity: ${:.2}\n\
             Total Return: {:.2}%\n\
             Max Drawdown: {:.2}%\n\
             Win Rate: {:.1}%\n\
             Total Trades: {}\n\
             Winning: {} | Losing: {}\n\
             Avg Win: ${:.2} | Avg Loss: ${:.2}\n\
             Profit Factor: {:.2}",
            result.initial_capital,
            result.final_equity,
            result.total_return * 100.0,
            result.max_drawdown * 100.0,
            result.win_rate * 100.0,
            result.total_trades,
            result.winning_trades,
            result.losing_trades,
            result.avg_win,
            result.avg_loss,
            result.profit_factor
        )
    }
}
