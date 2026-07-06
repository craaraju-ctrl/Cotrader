//! Strategy Selector — RandomForest for selecting the best strategy per market condition.
//!
//! Learns which strategy (StructureBreakout, TrendPullback, LiquiditySweep)
//! performs best in each market regime from historical outcomes.

use std::path::Path;

pub struct StrategySelectorModel {
    /// Strategy weights per feature bin (simplified RF approximation).
    /// In production, this would be a full RandomForest from linfa_trees.
    feature_importance: Vec<f64>,
    strategy_thresholds: Vec<Vec<f64>>,
    num_strategies: usize,
}

impl StrategySelectorModel {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let data = std::fs::read_to_string(path)?;
        let model_data: SelectorData = serde_json::from_str(&data)?;
        Ok(Self {
            feature_importance: model_data.feature_importance,
            strategy_thresholds: model_data.strategy_thresholds,
            num_strategies: model_data.num_strategies,
        })
    }

    /// Predict best strategy index and confidence.
    pub fn predict(&self, features: &[f64]) -> Result<(usize, f64), Box<dyn std::error::Error + Send + Sync>> {
        if self.strategy_thresholds.is_empty() {
            return Ok((0, 0.5));
        }

        // Weighted feature scoring for each strategy
        let mut scores = vec![0.0f64; self.num_strategies];
        for (strategy_idx, thresholds) in self.strategy_thresholds.iter().enumerate() {
            let mut score = 0.0;
            for (i, &threshold) in thresholds.iter().enumerate() {
                if i < features.len() && i < self.feature_importance.len() {
                    let feature_score = if features[i] > threshold { 1.0 } else { 0.0 };
                    score += feature_score * self.feature_importance[i];
                }
            }
            scores[strategy_idx] = score;
        }

        let total: f64 = scores.iter().sum();
        let (best_idx, &best_score) = scores.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, &0.0));

        let confidence = if total > 0.0 { best_score / total } else { 0.5 };

        Ok((best_idx, confidence))
    }

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let model_data = SelectorData {
            feature_importance: self.feature_importance.clone(),
            strategy_thresholds: self.strategy_thresholds.clone(),
            num_strategies: self.num_strategies,
        };
        let data = serde_json::to_string_pretty(&model_data)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SelectorData {
    feature_importance: Vec<f64>,
    strategy_thresholds: Vec<Vec<f64>>,
    num_strategies: usize,
}

/// Train a strategy selector from historical data.
pub fn train_strategy_selector(
    features_per_trade: &[Vec<f64>],
    strategy_used: &[usize],   // which strategy was used
    trade_outcomes: &[f64],    // PnL per trade
    num_strategies: usize,
    feature_count: usize,
) -> StrategySelectorModel {
    // For each strategy, compute average outcome per feature bin
    let mut strategy_thresholds = vec![vec![0.0; feature_count]; num_strategies];
    let mut feature_importance = vec![0.0f64; feature_count];

    for strat_idx in 0..num_strategies {
        let trades_for_strategy: Vec<(&Vec<f64>, &f64)> = features_per_trade.iter()
            .zip(trade_outcomes.iter())
            .zip(strategy_used.iter())
            .filter(|((_, _), &s)| s == strat_idx)
            .map(|((f, o), _)| (f, o))
            .collect();

        if trades_for_strategy.is_empty() {
            continue;
        }

        // For each feature, find the threshold that best separates wins from losses
        for feat_idx in 0..feature_count {
            let mut values: Vec<(f64, f64)> = trades_for_strategy.iter()
                .filter_map(|(f, &o)| f.get(feat_idx).map(|&v| (v, o)))
                .collect();

            if values.len() < 5 {
                continue;
            }

            values.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let median_idx = values.len() / 2;
            strategy_thresholds[strat_idx][feat_idx] = values[median_idx].0;

            // Feature importance = separation between win/loss means
            let wins: Vec<f64> = values.iter().filter(|(_, o)| *o > 0.0).map(|(v, _)| *v).collect();
            let losses: Vec<f64> = values.iter().filter(|(_, o)| *o <= 0.0).map(|(v, _)| *v).collect();
            if !wins.is_empty() && !losses.is_empty() {
                let win_mean = wins.iter().sum::<f64>() / wins.len() as f64;
                let loss_mean = losses.iter().sum::<f64>() / losses.len() as f64;
                feature_importance[feat_idx] += (win_mean - loss_mean).abs();
            }
        }
    }

    // Normalize feature importance
    let max_imp = feature_importance.iter().cloned().fold(0.0f64, f64::max);
    if max_imp > 0.0 {
        for imp in feature_importance.iter_mut() {
            *imp /= max_imp;
        }
    }

    StrategySelectorModel {
        feature_importance,
        strategy_thresholds,
        num_strategies,
    }
}
