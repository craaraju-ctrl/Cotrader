//! Portfolio Metrics — Sharpe, Sortino, drawdown calculations.

pub struct PortfolioMetrics;

impl PortfolioMetrics {
    pub fn sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
        if returns.is_empty() { return 0.0; }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();
        if std_dev == 0.0 { 0.0 } else { (mean - risk_free_rate) / std_dev }
    }

    pub fn sortino_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
        if returns.is_empty() { return 0.0; }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let downside_returns: Vec<f64> = returns.iter().filter(|&&r| r < risk_free_rate).map(|&r| r - risk_free_rate).collect();
        if downside_returns.is_empty() { return 0.0; }
        let downside_dev = (downside_returns.iter().map(|r| r.powi(2)).sum::<f64>() / downside_returns.len() as f64).sqrt();
        if downside_dev == 0.0 { 0.0 } else { (mean - risk_free_rate) / downside_dev }
    }

    pub fn max_drawdown(equity_curve: &[f64]) -> f64 {
        if equity_curve.is_empty() { return 0.0; }
        let mut peak = equity_curve[0];
        let mut max_dd = 0.0;
        for &equity in equity_curve {
            if equity > peak { peak = equity; }
            let dd = (peak - equity) / peak;
            if dd > max_dd { max_dd = dd; }
        }
        max_dd
    }

    pub fn win_rate(trades: &[(f64, f64)]) -> f64 {
        if trades.is_empty() { return 0.0; }
        let wins = trades.iter().filter(|(_, pnl)| *pnl > 0.0).count();
        wins as f64 / trades.len() as f64
    }

    pub fn profit_factor(trades: &[(f64, f64)]) -> f64 {
        let gross_profit: f64 = trades.iter().filter(|(_, pnl)| *pnl > 0.0).map(|(_, pnl)| pnl).sum();
        let gross_loss: f64 = trades.iter().filter(|(_, pnl)| *pnl < 0.0).map(|(_, pnl)| pnl.abs()).sum();
        if gross_loss == 0.0 { 0.0 } else { gross_profit / gross_loss }
    }
}
