//! Evaluator — Walk-forward validation and performance metrics.

/// Walk-forward validation result.
pub struct ValidationResult {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub samples: usize,
}

/// Compute classification metrics from predictions and labels.
pub fn classification_metrics(predictions: &[f64], labels: &[f64], threshold: f64) -> ValidationResult {
    let mut tp = 0.0;
    let mut fp = 0.0;
    let mut tn = 0.0;
    let mut fn_count = 0.0;

    for (&pred, &label) in predictions.iter().zip(labels.iter()) {
        let predicted_positive = pred > threshold;
        let actual_positive = label > 0.5;

        match (predicted_positive, actual_positive) {
            (true, true) => tp += 1.0,
            (true, false) => fp += 1.0,
            (false, false) => tn += 1.0,
            (false, true) => fn_count += 1.0,
        }
    }

    let total = tp + fp + tn + fn_count;
    let accuracy = if total > 0.0 { (tp + tn) / total } else { 0.0 };
    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if tp + fn_count > 0.0 { tp / (tp + fn_count) } else { 0.0 };
    let f1_score = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    ValidationResult {
        accuracy,
        precision,
        recall,
        f1_score,
        samples: predictions.len(),
    }
}

/// Compute Sharpe ratio from a series of returns.
pub fn sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
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

/// Maximum drawdown from an equity curve.
pub fn max_drawdown(equity_curve: &[f64]) -> f64 {
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

/// Walk-forward validation: train on data[0..split], test on data[split..], slide forward.
pub fn walk_forward_validate(
    all_features: &[Vec<f64>],
    all_labels: &[f64],
    train_window: usize,
    test_window: usize,
    train_fn: &dyn Fn(&[Vec<f64>], &[f64]) -> Vec<f64>, // returns predictions
) -> ValidationResult {
    let mut all_predictions = Vec::new();
    let mut all_actuals = Vec::new();

    let mut start = 0;
    while start + train_window + test_window <= all_features.len() {
        let train_features = &all_features[start..start + train_window];
        let train_labels = &all_labels[start..start + train_window];
        let _test_features = &all_features[start + train_window..start + train_window + test_window];
        let test_labels = &all_labels[start + train_window..start + train_window + test_window];

        let predictions = train_fn(train_features, train_labels);

        all_predictions.extend_from_slice(&predictions);
        all_actuals.extend_from_slice(test_labels);

        start += test_window; // slide forward by test window
    }

    classification_metrics(&all_predictions, &all_actuals, 0.5)
}
