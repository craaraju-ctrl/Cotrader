//! # Multi-Timeframe Confluence Scorer
//!
//! Scores signal confluence by checking alignment across timeframes.
//! Higher timeframes carry more weight because they represent stronger trends.
//!
//! Weights:
//! - D1:  30% (dominant trend)
//! - H4:  25% (medium-term momentum)
//! - H1:  20% (intraday trend)
//! - M15: 15% (short-term entry timing)
//! - M5:   7% (micro entry precision)
//! - M1:   3% (noise — minimal weight)
//!
//! A high confluence score means multiple timeframes agree on direction.

use crate::multi_timeframe_analyst::{MultiTimeframeResult, Timeframe};

/// Confluence result for a symbol
#[derive(Debug, Clone)]
pub struct ConfluenceResult {
    pub symbol: String,
    pub alignment_score: f64,  // -1.0 to 1.0 (weighted average)
    pub alignment_pct: f64,    // 0.0 to 100.0 (percentage of timeframes agreeing)
    pub direction: ConfluenceDirection,
    pub timeframe_agreement: Vec<(Timeframe, f64)>,
    pub strongest_tf: Option<Timeframe>,
    pub weakest_tf: Option<Timeframe>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfluenceDirection {
    StrongBuy,   // alignment > 0.6
    Buy,         // alignment > 0.3
    Neutral,     // alignment between -0.3 and 0.3
    Sell,        // alignment < -0.3
    StrongSell,  // alignment < -0.6
}

/// Multi-Timeframe Confluence Scorer
pub struct MtfConfluenceScorer {
    /// Timeframe weights (must sum to 1.0)
    weights: Vec<(Timeframe, f64)>,
    /// Minimum alignment threshold for a trade signal
    min_alignment: f64,
}

impl Default for MtfConfluenceScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl MtfConfluenceScorer {
    pub fn new() -> Self {
        Self {
            weights: vec![
                (Timeframe::M1, 0.03),
                (Timeframe::M5, 0.07),
                (Timeframe::M15, 0.15),
                (Timeframe::H1, 0.20),
                (Timeframe::H4, 0.25),
                (Timeframe::D1, 0.30),
            ],
            min_alignment: 0.4,
        }
    }

    /// Score confluence from a multi-timeframe analysis result
    pub fn score(&self, mtf: &MultiTimeframeResult) -> ConfluenceResult {
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        let mut agreeing = 0;
        let mut total_tf = 0;
        let mut strongest_score = f64::NEG_INFINITY;
        let mut weakest_score = f64::INFINITY;
        let mut strongest_tf = None;
        let mut weakest_tf = None;
        let direction_from = mtf.aggregate_score;

        let mut timeframe_agreement = Vec::new();

        for tf_ind in &mtf.timeframes {
            if let Some(&(_, weight)) = self.weights.iter().find(|(tf, _)| *tf == tf_ind.timeframe) {
                weighted_sum += tf_ind.score * weight;
                total_weight += weight;
                timeframe_agreement.push((tf_ind.timeframe, tf_ind.score));

                // Check agreement with aggregate
                let agrees = (tf_ind.score > 0.0 && direction_from > 0.0)
                    || (tf_ind.score < 0.0 && direction_from < 0.0)
                    || (tf_ind.score.abs() < 0.1 && direction_from.abs() < 0.1);
                if agrees { agreeing += 1; }
                total_tf += 1;

                if tf_ind.score > strongest_score {
                    strongest_score = tf_ind.score;
                    strongest_tf = Some(tf_ind.timeframe);
                }
                if tf_ind.score < weakest_score {
                    weakest_score = tf_ind.score;
                    weakest_tf = Some(tf_ind.timeframe);
                }
            }
        }

        let alignment_score = if total_weight > 0.0 { weighted_sum / total_weight } else { 0.0 };
        let alignment_pct = if total_tf > 0 {
            (agreeing as f64 / total_tf as f64) * 100.0
        } else {
            0.0
        };

        let direction = if alignment_score > 0.6 {
            ConfluenceDirection::StrongBuy
        } else if alignment_score > 0.3 {
            ConfluenceDirection::Buy
        } else if alignment_score < -0.6 {
            ConfluenceDirection::StrongSell
        } else if alignment_score < -0.3 {
            ConfluenceDirection::Sell
        } else {
            ConfluenceDirection::Neutral
        };

        ConfluenceResult {
            symbol: mtf.symbol.clone(),
            alignment_score,
            alignment_pct,
            direction,
            timeframe_agreement,
            strongest_tf,
            weakest_tf,
        }
    }

    /// Check if confluence is strong enough for a trade
    pub fn is_tradeable(&self, result: &ConfluenceResult) -> bool {
        result.alignment_score.abs() >= self.min_alignment && result.alignment_pct >= 50.0
    }

    /// Get recommended position size multiplier based on confluence strength
    /// Returns 0.0 to 1.0
    pub fn position_multiplier(&self, result: &ConfluenceResult) -> f64 {
        let strength = result.alignment_score.abs();
        let agreement = result.alignment_pct / 100.0;
        // Combine strength and agreement
        let base = strength * 0.6 + agreement * 0.4;
        // Scale: 0.0 at min_alignment, 1.0 at 1.0
        ((base - self.min_alignment) / (1.0 - self.min_alignment)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_timeframe_analyst::TimeframeIndicators;

    fn make_mtf_result(scores: Vec<(Timeframe, f64)>) -> MultiTimeframeResult {
        let timeframes: Vec<TimeframeIndicators> = scores
            .into_iter()
            .map(|(tf, score)| TimeframeIndicators {
                timeframe: tf,
                rsi: 50.0,
                macd: 0.0,
                macd_signal: 0.0,
                adx: 20.0,
                bb_upper: 100.0,
                bb_lower: 90.0,
                bb_position: 0.5,
                stoch_k: 50.0,
                stoch_d: 50.0,
                score,
            })
            .collect();
        let aggregate = timeframes.iter().map(|t| t.score).sum::<f64>() / timeframes.len() as f64;
        MultiTimeframeResult {
            symbol: "BTC".to_string(),
            timeframes,
            aggregate_score: aggregate,
        }
    }

    #[test]
    fn test_strong_bullish_confluence() {
        let scorer = MtfConfluenceScorer::new();
        let mtf = make_mtf_result(vec![
            (Timeframe::M1, 0.8),
            (Timeframe::M5, 0.7),
            (Timeframe::M15, 0.9),
            (Timeframe::H1, 0.8),
            (Timeframe::H4, 0.9),
            (Timeframe::D1, 0.9),
        ]);
        let result = scorer.score(&mtf);
        assert!(result.alignment_score > 0.7, "Expected strong bullish, got {}", result.alignment_score);
        assert_eq!(result.direction, ConfluenceDirection::StrongBuy);
        assert!(scorer.is_tradeable(&result));
    }

    #[test]
    fn test_mixed_signals() {
        let scorer = MtfConfluenceScorer::new();
        let mtf = make_mtf_result(vec![
            (Timeframe::M1, 0.8),
            (Timeframe::M5, -0.5),
            (Timeframe::M15, 0.3),
            (Timeframe::H1, -0.7),
            (Timeframe::H4, 0.4),
            (Timeframe::D1, -0.2),
        ]);
        let result = scorer.score(&mtf);
        assert!(result.alignment_score.abs() < 0.5, "Mixed signals should have low alignment");
    }

    #[test]
    fn test_position_multiplier() {
        let scorer = MtfConfluenceScorer::new();
        let mtf = make_mtf_result(vec![
            (Timeframe::M1, 0.0),
            (Timeframe::M5, 0.0),
            (Timeframe::M15, 0.0),
            (Timeframe::H1, 0.0),
            (Timeframe::H4, 0.0),
            (Timeframe::D1, 0.0),
        ]);
        let result = scorer.score(&mtf);
        let mult = scorer.position_multiplier(&result);
        assert!(mult >= 0.0 && mult <= 1.0);
    }
}
