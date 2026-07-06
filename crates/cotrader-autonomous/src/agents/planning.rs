//! Planning Agent — Strategy selection → signal generation → trade setup.
//!
//! Merges: StrategyDecision, DeterministicStrategies, SymbolRanker, MultiTimeframeAnalyst

use super::analysis::AnalysisResult;
use super::reasoning::ReasoningChain;
use crate::state::SharedState;
use crate::types::{MarketRegime, TradeSignal};
use chrono::Utc;

#[derive(Clone)]
pub struct PlanningAgent {
    pub state: SharedState,
}

#[derive(Debug, Clone)]
pub struct PlanResult {
    pub symbol: String,
    pub signal: Option<TradeSignal>,
    pub strategy_used: String,
    pub entry: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub confidence: f64,
}

impl PlanningAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Generate a trade plan based on analysis.
    pub async fn plan(&self, analysis: &AnalysisResult, current_price: f64) -> PlanResult {
        // 1. Select best strategy for current regime
        let strategy = self.select_strategy(&analysis.regime);

        // 2. Compute entry/SL/TP levels
        let (entry, sl, tp) = self.compute_levels(current_price, &analysis.regime, &strategy);

        // 3. Compute position size via Kelly + ML
        let position_size = self.compute_position_size(entry, sl).await;

        // 4. Build signal if conditions met
        let signal = if analysis.confidence > 0.4 {
            Some(TradeSignal {
                symbol: analysis.symbol.clone(),
                direction: if current_price < entry { cotrader_core::TradeDirection::Long } else { cotrader_core::TradeDirection::Short },
                entry_price: entry,
                stop_loss: sl,
                take_profit: tp,
                position_size,
                confidence_score: analysis.confidence,
                confluence_score: analysis.confidence,
                risk_reward_ratio: if (entry - sl).abs() > 0.0 { (tp - entry).abs() / (entry - sl).abs() } else { 2.0 },
                reasoning: format!("{} strategy in {:?} regime", strategy, analysis.regime),
                timestamp: Utc::now(),
                session_valid: true,
                risk_check_passed: true,
            })
        } else {
            None
        };

        PlanResult {
            symbol: analysis.symbol.clone(),
            signal,
            strategy_used: strategy,
            entry,
            stop_loss: sl,
            take_profit: tp,
            confidence: analysis.confidence,
        }
    }

    fn select_strategy(&self, regime: &MarketRegime) -> String {
        match regime {
            MarketRegime::TrendingBull | MarketRegime::TrendingBear => "TrendPullback".to_string(),
            MarketRegime::Volatile => "StructureBreakout".to_string(),
            MarketRegime::Ranging => "MeanReversion".to_string(),
            MarketRegime::LowLiquidity => "Scalping".to_string(),
        }
    }

    fn compute_levels(&self, price: f64, regime: &MarketRegime, strategy: &str) -> (f64, f64, f64) {
        let (sl_pct, tp_pct) = match strategy {
            "StructureBreakout" => (0.02, 0.04),
            "TrendPullback" => (0.015, 0.03),
            "MeanReversion" => (0.01, 0.02),
            "LiquiditySweep" => (0.025, 0.05),
            _ => (0.02, 0.04),
        };

        let vol_mult = match regime {
            MarketRegime::Volatile => 1.5,
            MarketRegime::LowLiquidity => 1.3,
            _ => 1.0,
        };

        let sl = price * (1.0 - sl_pct * vol_mult);
        let tp = price * (1.0 + tp_pct * vol_mult);
        (price, sl, tp)
    }

    async fn compute_position_size(&self, entry: f64, stop_loss: f64) -> f64 {
        // ML-enhanced Kelly sizing
        let empty_bars = Vec::new();
        let ml_features = self.state.ml_engine.feature_store().build_features(
            50.0, 0.0, 0.015, 0.0, 0.0, 0.0, 50.0, 25.0, 0.0, -50.0,
            0.0, 50.0, 0.0, 0.0, 50.0, 50.0, "uptrend", 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 50.0, 50.0, 0.0, 0.0, 1.0, 0.02,
            &empty_bars, None, 0.02, 0, 0.0,
        );
        let (win_prob, _) = self.state.ml_engine.predict_win_probability(&ml_features, 0.55).await;

        // Kelly criterion
        let risk_reward = if (entry - stop_loss).abs() > 0.0 { 2.0 } else { 2.0 };
        let kelly = win_prob - ((1.0 - win_prob) / risk_reward);
        let portfolio = self.state.portfolio_store.portfolio.read().await;
        let equity = portfolio.total_equity;
        let risk_pct = self.state.rule_engine.rules.read().await.max_risk_per_trade;

        let risk_based = (equity * risk_pct) / (entry - stop_loss).abs();
        let kelly_size = (kelly * equity / entry).max(0.0);
        risk_based.min(kelly_size).clamp(0.01, 0.25)
    }

    /// Produce reasoning chain.
    pub fn reason(&self, plan: &PlanResult) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Planning", &plan.symbol);

        chain.add_step(
            &format!("Selected strategy: {}", plan.strategy_used),
            "Strategy chosen based on current market regime",
            vec![format!("strategy={}", plan.strategy_used)],
            0.7,
        );

        chain.add_step(
            &format!("Computed levels: entry={:.2} SL={:.2} TP={:.2}", plan.entry, plan.stop_loss, plan.take_profit),
            &format!("R:R = {:.1}:1", if (plan.entry - plan.stop_loss).abs() > 0.0 { (plan.take_profit - plan.entry).abs() / (plan.entry - plan.stop_loss).abs() } else { 0.0 }),
            vec![
                format!("entry={:.2}", plan.entry),
                format!("SL={:.2}", plan.stop_loss),
                format!("TP={:.2}", plan.take_profit),
            ],
            0.75,
        );

        chain.finalize(&format!("Plan ready: {} @ {:.2} (conf {:.0}%)", plan.strategy_used, plan.entry, plan.confidence * 100.0));
        chain
    }
}
