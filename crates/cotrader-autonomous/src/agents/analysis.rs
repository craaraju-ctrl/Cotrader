//! Analysis Agent — Market data → indicators → patterns → regime → NLP sentiment.
//!
//! Refactored to consume `CacheFrame` instead of `SharedState`.
//! Side effects are emitted via channel sender.

use super::reasoning::ReasoningChain;
use crate::market_metrics_meter::MetricsSnapshot;
use crate::types::{AgentOutputEvent, CacheFrame, MarketRegime};
use std::sync::Arc;

#[derive(Clone)]
pub struct AnalysisAgent {
    pub ml_engine: Arc<cotrader_ml::MLEngine>,
    pub cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
}

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub symbol: String,
    pub metrics: Option<MetricsSnapshot>,
    pub regime: MarketRegime,
    pub patterns: Vec<String>,
    pub sentiment: Option<NlpSentiment>,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct NlpSentiment {
    pub score: f64,
    pub label: String,
    pub reasoning: String,
}

impl AnalysisAgent {
    pub fn new(
        ml_engine: Arc<cotrader_ml::MLEngine>,
        cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
    ) -> Self {
        Self { ml_engine, cot_tx }
    }

    /// Full analysis from a CacheFrame: compute indicators, detect regime, find patterns, NLP sentiment.
    pub async fn analyze(&self, frame: &CacheFrame) -> AnalysisResult {
        let symbol = &frame.symbol;
        let current_price = frame.current_price;

        // 1. Metrics from frame (pre-computed by Core Engine)
        let metrics = frame.metrics.clone();

        // 2. Detect regime using frame data + ML engine
        let regime = self.detect_regime(frame).await;

        // 3. Patterns from frame
        let patterns = frame.patterns.clone();

        // 4. NLP sentiment from news in frame
        let sentiment = self.analyze_sentiment_nlp(frame).await;

        // 5. Compute confidence
        let confidence = self.compute_confidence_with_sentiment(&metrics, &regime, &patterns, &sentiment);

        // Emit COT event
        let _ = self
            .cot_tx
            .send(AgentOutputEvent::Cot {
                agent: "Analysis".to_string(),
                symbol: symbol.clone(),
                action: "ANALYZED".to_string(),
                reason: format!(
                    "Regime: {:?}, sentiment: {}, conf: {:.0}%",
                    regime,
                    sentiment.as_ref().map(|s| s.label.as_str()).unwrap_or("N/A"),
                    confidence * 100.0
                ),
                confidence,
            });

        AnalysisResult {
            symbol: symbol.to_string(),
            metrics,
            regime,
            patterns,
            sentiment,
            confidence,
        }
    }

    /// NLP-enhanced sentiment analysis using keyword-based scoring.
    async fn analyze_sentiment_nlp(&self, frame: &CacheFrame) -> Option<NlpSentiment> {
        let headlines: Vec<String> = frame
            .latest_news
            .as_ref()
            .map(|ctx| ctx.headlines.iter().map(|h| h.title.clone()).collect())
            .unwrap_or_default();

        if headlines.is_empty() {
            return None;
        }

        let combined = headlines.join(" ").to_lowercase();
        let bullish_words = [
            "bull", "surge", "rally", "gain", "rise", "buy", "up", "positive", "growth",
        ];
        let bearish_words = [
            "bear", "crash", "drop", "fall", "loss", "sell", "down", "negative", "decline",
        ];

        let bull_count = bullish_words.iter().filter(|w| combined.contains(*w)).count() as f64;
        let bear_count = bearish_words.iter().filter(|w| combined.contains(*w)).count() as f64;
        let total = bull_count + bear_count;

        let score = if total > 0.0 {
            (bull_count - bear_count) / total
        } else {
            0.0
        };
        let label = if score > 0.2 {
            "bullish"
        } else if score < -0.2 {
            "bearish"
        } else {
            "neutral"
        };
        let reasoning = format!(
            "Keyword analysis: {} bullish, {} bearish words",
            bull_count as i32, bear_count as i32
        );

        Some(NlpSentiment {
            score,
            label: label.to_string(),
            reasoning,
        })
    }

    /// Compute confidence with sentiment boost.
    fn compute_confidence_with_sentiment(
        &self,
        metrics: &Option<MetricsSnapshot>,
        regime: &MarketRegime,
        patterns: &[String],
        sentiment: &Option<NlpSentiment>,
    ) -> f64 {
        let mut conf = self.compute_confidence(metrics, regime, patterns);

        if let Some(ref sent) = sentiment {
            if sent.score.abs() > 0.5 {
                conf += 0.1;
            }
            if sent.label == "mixed" {
                conf -= 0.05;
            }
        }

        conf.clamp(0.0, 1.0)
    }

    /// ML-enhanced regime detection from CacheFrame data.
    async fn detect_regime(&self, frame: &CacheFrame) -> MarketRegime {
        let price = frame.current_price;
        let bars = &frame.ohlcv_bars;

        if bars.len() < 20 {
            return MarketRegime::Ranging;
        }
        let recent = &bars[bars.len() - 10..];
        let vol: f64 = recent
            .windows(2)
            .map(|w| (w[1].close - w[0].close).abs() / w[0].close)
            .sum::<f64>()
            / 9.0;
        let slope =
            (price - bars[bars.len() - 10].close) / bars[bars.len() - 10].close;

        let empty_bars = Vec::new();
        let ml_features = self.ml_engine.feature_store().build_features(
            50.0, 0.0, 0.015, 0.0, 0.0, 0.0, 50.0, 25.0, 0.0, -50.0, 0.0, 50.0, 0.0, 0.0,
            50.0, 50.0, "uptrend", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 50.0, 50.0, 0.0, 0.0,
            1.0, 0.02, &empty_bars, None, vol, 0, 0.0,
        );
        let (ml_regime, ml_conf, ml_source) = self
            .ml_engine
            .predict_regime(&ml_features, MarketRegime::Ranging)
            .await;

        if ml_source == "ml" && ml_conf > 0.6 {
            return ml_regime;
        }

        // Fallback to threshold
        if vol > 0.025 {
            MarketRegime::Volatile
        } else if slope > 0.02 {
            MarketRegime::TrendingBull
        } else if slope < -0.02 {
            MarketRegime::TrendingBear
        } else {
            MarketRegime::Ranging
        }
    }

    fn compute_confidence(
        &self,
        metrics: &Option<MetricsSnapshot>,
        regime: &MarketRegime,
        patterns: &[String],
    ) -> f64 {
        let mut conf: f64 = 0.5;
        if let Some(m) = metrics {
            if m.rsi_14 > 70.0 || m.rsi_14 < 30.0 {
                conf += 0.1;
            }
            if m.adx > 25.0 {
                conf += 0.1;
            }
            if m.confluence_hint > 0.6 {
                conf += 0.1;
            }
        }
        match regime {
            MarketRegime::TrendingBull | MarketRegime::TrendingBear => conf += 0.1,
            MarketRegime::Volatile => conf -= 0.05,
            _ => {}
        }
        if !patterns.is_empty() {
            conf += 0.05;
        }
        conf.clamp(0.0, 1.0)
    }

    /// Produce reasoning chain.
    pub fn reason(&self, result: &AnalysisResult) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Analysis", &result.symbol);

        if let Some(ref m) = result.metrics {
            chain.add_step(
                &format!("Computed 26+ indicators for {}", result.symbol),
                &format!(
                    "RSI={:.1}, MACD_hist={:.4}, ADX={:.1}, ATR%={:.2}%",
                    m.rsi_14,
                    m.macd_hist,
                    m.adx,
                    m.atr_pct * 100.0
                ),
                vec![
                    format!("RSI={:.1}", m.rsi_14),
                    format!("MACD={:.4}", m.macd_hist),
                    format!("ADX={:.1}", m.adx),
                ],
                if (m.rsi_14 - 50.0).abs() > 20.0 {
                    0.8
                } else {
                    0.6
                },
            );
        }

        chain.add_step(
            &format!("Detected regime: {:?}", result.regime),
            match result.regime {
                MarketRegime::TrendingBull => "Price trending up with momentum — favor longs",
                MarketRegime::TrendingBear => "Price trending down — favor shorts or cash",
                MarketRegime::Ranging => "No clear trend — range-bound strategies",
                MarketRegime::Volatile => "High volatility — wider stops, smaller positions",
                MarketRegime::LowLiquidity => "Low liquidity — slippage risk elevated",
            },
            vec![format!("regime={:?}", result.regime)],
            0.7,
        );

        if !result.patterns.is_empty() {
            chain.add_step(
                &format!("Detected {} patterns", result.patterns.len()),
                &format!("Patterns found: {}", result.patterns.join(", ")),
                result.patterns.iter().cloned().collect(),
                0.65,
            );
        }

        if let Some(ref sent) = result.sentiment {
            chain.add_step(
                &format!("NLP Sentiment: {} ({:.2})", sent.label, sent.score),
                &sent.reasoning,
                vec![
                    format!("sentiment={:.2}", sent.score),
                    format!("label={}", sent.label),
                ],
                0.75,
            );
        }

        chain.finalize(&format!(
            "Analysis: regime={:?}, sentiment={}, conf={:.0}%",
            result.regime,
            result
                .sentiment
                .as_ref()
                .map(|s| s.label.as_str())
                .unwrap_or("N/A"),
            result.confidence * 100.0
        ));
        chain
    }
}
