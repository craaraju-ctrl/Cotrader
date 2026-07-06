//! Cross-Asset Risk Engine — Portfolio-level risk management.
//!
//! For a world trading system, risk must be managed across:
//! - Multiple asset classes (equity, crypto, forex, commodities)
//! - Multiple currencies (USD, INR, JPY, EUR)
//! - Multiple exchanges (Binance, NSE, NYSE, TSE)
//!
//! Key features:
//! - Value at Risk (VaR) across all positions
//! - Currency exposure tracking
//! - Correlation-based risk reduction
//! - Concentration limits per asset class
//! - Margin utilization tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::asset_class::AssetCategory;

/// Portfolio-level risk metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioRisk {
    /// Total portfolio value in base currency (USD)
    pub total_value: f64,
    /// Value at Risk (95% confidence, 1-day)
    pub var_95: f64,
    /// Value at Risk (99% confidence, 1-day)
    pub var_99: f64,
    /// Expected Shortfall (CVaR) at 95%
    pub cvar_95: f64,
    /// Maximum drawdown from peak
    pub max_drawdown: f64,
    /// Sharpe ratio (annualized)
    pub sharpe_ratio: f64,
    /// Portfolio beta (vs benchmark)
    pub beta: f64,
}

/// Position-level risk data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRisk {
    pub symbol: String,
    pub asset_class: AssetCategory,
    pub currency: String,
    pub exchange: String,
    pub position_value: f64,
    pub position_value_base: f64, // In base currency (USD)
    pub weight: f64, // % of portfolio
    pub contribution_to_var: f64,
    pub correlation_to_portfolio: f64,
}

/// Currency exposure breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyExposure {
    pub currency: String,
    pub long_exposure: f64,
    pub short_exposure: f64,
    pub net_exposure: f64,
    pub hedge_ratio: f64,
}

/// Cross-asset risk engine.
pub struct CrossAssetRiskEngine {
    /// Base currency for all risk calculations
    pub base_currency: String,
    /// Max portfolio value per asset class (% of total)
    pub concentration_limits: HashMap<AssetCategory, f64>,
    /// Max correlation between any two positions
    pub max_correlation: f64,
    /// Current forex rates (currency → USD rate)
    pub forex_rates: HashMap<String, f64>,
}

impl CrossAssetRiskEngine {
    /// Create a new risk engine with default limits.
    pub fn new(base_currency: &str) -> Self {
        let mut concentration_limits = HashMap::new();
        concentration_limits.insert(AssetCategory::Equity, 0.40);    // 40% max in equities
        concentration_limits.insert(AssetCategory::Crypto, 0.20);    // 20% max in crypto
        concentration_limits.insert(AssetCategory::Forex, 0.25);     // 25% max in forex
        concentration_limits.insert(AssetCategory::Commodity, 0.25); // 25% max in commodities
        concentration_limits.insert(AssetCategory::Derivative, 0.15); // 15% max in derivatives

        let mut forex_rates = HashMap::new();
        forex_rates.insert("USD".to_string(), 1.0);
        forex_rates.insert("INR".to_string(), 0.012); // 1 INR ≈ 0.012 USD
        forex_rates.insert("JPY".to_string(), 0.0067); // 1 JPY ≈ 0.0067 USD
        forex_rates.insert("EUR".to_string(), 1.08);   // 1 EUR ≈ 1.08 USD
        forex_rates.insert("GBP".to_string(), 1.27);   // 1 GBP ≈ 1.27 USD

        Self {
            base_currency: base_currency.to_string(),
            concentration_limits,
            max_correlation: 0.7, // Max 70% correlation between positions
            forex_rates,
        }
    }

    /// Update forex rates.
    pub fn update_forex_rates(&mut self, rates: HashMap<String, f64>) {
        self.forex_rates.extend(rates);
    }

    /// Convert amount from one currency to base currency.
    pub fn convert_to_base(&self, amount: f64, from_currency: &str) -> f64 {
        if from_currency == self.base_currency {
            return amount;
        }
        if let Some(&rate) = self.forex_rates.get(from_currency) {
            amount * rate
        } else {
            0.0 // Unknown currency
        }
    }

    /// Calculate concentration risk for a set of positions.
    pub fn check_concentration(&self, positions: &[PositionRisk]) -> Vec<String> {
        let mut warnings = Vec::new();

        // Group by asset class
        let mut by_category: HashMap<AssetCategory, f64> = HashMap::new();
        for pos in positions {
            *by_category.entry(pos.asset_class.clone()).or_insert(0.0) += pos.position_value_base;
        }

        let total: f64 = positions.iter().map(|p| p.position_value_base).sum();

        for (category, value) in &by_category {
            let weight = if total > 0.0 { value / total } else { 0.0 };
            let limit = self.concentration_limits.get(category).unwrap_or(&0.30);

            if weight > *limit {
                warnings.push(format!(
                    "Concentration risk: {:?} at {:.1}% (limit {:.1}%)",
                    category,
                    weight * 100.0,
                    limit * 100.0
                ));
            }
        }

        warnings
    }

    /// Calculate portfolio VaR using historical simulation.
    pub fn calculate_var(&self, returns: &[f64], confidence: f64) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }

        let mut sorted_returns = returns.to_vec();
        sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let index = ((1.0 - confidence) * sorted_returns.len() as f64) as usize;
        let index = index.min(sorted_returns.len() - 1);

        -sorted_returns[index] // VaR is positive loss
    }

    /// Calculate Sharpe ratio.
    pub fn calculate_sharpe(&self, returns: &[f64], risk_free_rate: f64) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }

        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();

        if std_dev > 0.0 {
            (mean - risk_free_rate) / std_dev
        } else {
            0.0
        }
    }

    /// Calculate maximum drawdown from equity curve.
    pub fn calculate_max_drawdown(&self, equity_curve: &[f64]) -> f64 {
        if equity_curve.is_empty() {
            return 0.0;
        }

        let mut peak = equity_curve[0];
        let mut max_dd = 0.0;

        for &value in equity_curve {
            if value > peak {
                peak = value;
            }
            let dd = (peak - value) / peak;
            if dd > max_dd {
                max_dd = dd;
            }
        }

        max_dd
    }

    /// Full risk report for a set of positions.
    pub fn full_risk_report(
        &self,
        positions: &[PositionRisk],
        returns: &[f64],
        equity_curve: &[f64],
    ) -> RiskReport {
        let total_value: f64 = positions.iter().map(|p| p.position_value_base).sum();

        let var_95 = self.calculate_var(returns, 0.95);
        let var_99 = self.calculate_var(returns, 0.99);
        let cvar_95 = self.calculate_var(returns, 0.95) * 1.2; // Simplified CVaR
        let max_drawdown = self.calculate_max_drawdown(equity_curve);
        let sharpe = self.calculate_sharpe(returns, 0.05 / 252.0); // Daily risk-free

        let concentration_warnings = self.check_concentration(positions);

        RiskReport {
            portfolio_risk: PortfolioRisk {
                total_value,
                var_95,
                var_99,
                cvar_95,
                max_drawdown,
                sharpe_ratio: sharpe,
                beta: 1.0, // Would calculate vs benchmark
            },
            positions: positions.to_vec(),
            concentration_warnings,
            recommendations: self.generate_recommendations(positions, var_95, max_drawdown),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Generate risk recommendations.
    fn generate_recommendations(
        &self,
        positions: &[PositionRisk],
        var_95: f64,
        max_drawdown: f64,
    ) -> Vec<String> {
        let mut recs = Vec::new();

        if var_95 > 0.03 {
            recs.push("VaR > 3% — consider reducing position sizes".to_string());
        }
        if max_drawdown > 0.10 {
            recs.push("Drawdown > 10% — review strategy performance".to_string());
        }

        // Check for high concentration
        let warnings = self.check_concentration(positions);
        recs.extend(warnings);

        recs
    }
}

/// Complete risk report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskReport {
    pub portfolio_risk: PortfolioRisk,
    pub positions: Vec<PositionRisk>,
    pub concentration_warnings: Vec<String>,
    pub recommendations: Vec<String>,
    pub timestamp: String,
}
