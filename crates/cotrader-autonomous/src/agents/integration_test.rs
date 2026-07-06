//! Integration test: Verify 8-agent types and reasoning work together.

#[cfg(test)]
mod tests {
    use super::super::analysis::{AnalysisResult, NlpSentiment};
    use super::super::planning::PlanResult;
    use super::super::decision::DecisionResult;
    use super::super::observation::ObservationSummary;
    use super::super::risk;
    use super::super::psychology;
    use super::super::evolution;
    use super::super::reasoning::ReasoningChain;

    #[test]
    fn test_reasoning_chain_creation() {
        let mut chain = ReasoningChain::new("Analysis", "BTC");
        chain.add_step("Computed indicators", "RSI=65, MACD=0.002", vec!["rsi=65".to_string()], 0.8);
        chain.add_step("Detected regime", "TrendingBull", vec!["regime=TrendingBull".to_string()], 0.75);
        chain.finalize("Analysis complete with bullish signals");

        assert_eq!(chain.agent, "Analysis");
        assert_eq!(chain.symbol, "BTC");
        assert_eq!(chain.steps.len(), 2);
        assert_eq!(chain.conclusion, "Analysis complete with bullish signals");
        assert!(chain.confidence > 0.0);
    }

    #[test]
    fn test_reasoning_chain_format() {
        let mut chain = ReasoningChain::new("Decision", "ETH");
        chain.add_step("ML scoring", "P(profitable)=72%", vec!["ml=0.72".to_string()], 0.8);
        chain.finalize("BUY signal verified");

        let log = chain.format_for_log();
        assert!(log.contains("Decision"));
        assert!(log.contains("ETH"));
        assert!(log.contains("ML scoring"));
        assert!(log.contains("BUY signal verified"));
    }

    #[test]
    fn test_analysis_result_structure() {
        let result = AnalysisResult {
            symbol: "SOL".to_string(),
            metrics: None,
            regime: cotrader_core::MarketRegime::Volatile,
            patterns: vec!["Hammer".to_string(), "Doji".to_string()],
            sentiment: Some(NlpSentiment {
                score: 0.3,
                label: "bullish".to_string(),
                reasoning: "News looks positive".to_string(),
            }),
            confidence: 0.72,
        };

        assert_eq!(result.symbol, "SOL");
        assert_eq!(result.patterns.len(), 2);
        assert!(result.confidence > 0.5);
        assert!(result.sentiment.is_some());
    }

    #[test]
    fn test_plan_result_structure() {
        let plan = PlanResult {
            symbol: "BTC".to_string(),
            signal: None,
            strategy_used: "StructureBreakout".to_string(),
            entry: 58500.0,
            stop_loss: 57000.0,
            take_profit: 61500.0,
            confidence: 0.68,
        };

        assert_eq!(plan.strategy_used, "StructureBreakout");
        assert!(plan.take_profit > plan.entry);
        assert!(plan.stop_loss < plan.entry);
    }

    #[test]
    fn test_decision_result_with_neurosymbolic() {
        let decision = DecisionResult {
            symbol: "ETH".to_string(),
            action: "BUY".to_string(),
            confidence: 0.78,
            conviction: 0.75,
            ml_score: 0.82,
            verified: true,
            neurosymbolic_verified: true,
            reasoning: "ML=82% | Rules OK | Kronos confirms".to_string(),
        };

        assert!(decision.neurosymbolic_verified);
        assert!(decision.ml_score > 0.5);
        assert!(decision.action == "BUY");
    }

    #[test]
    fn test_risk_check_result() {
        let passed = risk::RiskCheckResult {
            passed: true,
            blocking_reason: None,
            warnings: vec![],
            risk_score: 0.025,
            position_size_allowed: 0.12,
            adjustments: risk::RiskAdjustments::default(),
        };

        let blocked = risk::RiskCheckResult {
            passed: false,
            blocking_reason: Some("Portfolio heat too high".to_string()),
            warnings: vec!["Heat elevated".to_string()],
            risk_score: 0.12,
            position_size_allowed: 0.0,
            adjustments: risk::RiskAdjustments::default(),
        };

        assert!(passed.passed);
        assert!(!blocked.passed);
        assert!(blocked.blocking_reason.is_some());
    }

    #[test]
    fn test_psychology_states() {
        let clean = psychology::PsychologyState {
            biases_detected: vec![],
            discipline_score: 0.92,
            adjustments: vec![],
            emotional_state: psychology::EmotionalState::Calm,
        };

        let biased = psychology::PsychologyState {
            biases_detected: vec![psychology::Bias {
                name: "Revenge Trading".to_string(),
                severity: 0.8,
                evidence: "3 consecutive losses".to_string(),
                recommendation: "Pause trading".to_string(),
            }],
            discipline_score: 0.45,
            adjustments: vec!["Reduce position size 50%".to_string()],
            emotional_state: psychology::EmotionalState::Frustrated,
        };

        assert!(clean.biases_detected.is_empty());
        assert!(clean.discipline_score > 0.8);
        assert!(!biased.biases_detected.is_empty());
        assert!(biased.discipline_score < 0.5);
    }

    #[test]
    fn test_observation_summary() {
        let summary = ObservationSummary {
            total_trades: 100,
            win_rate: 0.58,
            avg_regret: 0.12,
            recent_outcome: Some("WIN".to_string()),
            rules_discovered: 5,
        };

        assert_eq!(summary.total_trades, 100);
        assert!(summary.win_rate > 0.5);
        assert!(summary.rules_discovered > 0);
    }

    #[test]
    fn test_evolution_status() {
        let status = evolution::EvolutionStatus {
            episodes_collected: 250,
            models_deployed: vec![evolution::ModelInfo {
                name: "win_probability.json".to_string(),
                version: "1.0".to_string(),
                accuracy: 0.65,
                last_trained: "2h ago".to_string(),
            }],
            weight_adjustments: 12,
            last_improvement: Some("Win rate improved 3%".to_string()),
            training_queue_depth: 0,
            next_retrain_in: 3600,
        };

        assert!(status.episodes_collected > 100);
        assert_eq!(status.models_deployed.len(), 1);
    }
}
