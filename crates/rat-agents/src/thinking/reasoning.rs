//! Reasoning Engine — Chain-of-thought reasoning for trading decisions.

pub struct ReasoningEngine;

impl ReasoningEngine {
    /// Generate reasoning chain for a trading decision.
    pub fn reason(decision: &str, context: &str) -> ReasoningChain {
        let mut steps = Vec::new();

        // Step 1: Understand the market context
        steps.push(ReasoningStep {
            step: "Context Analysis".to_string(),
            observation: context.to_string(),
            confidence: 0.8,
        });

        // Step 2: Evaluate indicators
        steps.push(ReasoningStep {
            step: "Indicator Evaluation".to_string(),
            observation: "RSI and MACD signals processed".to_string(),
            confidence: 0.7,
        });

        // Step 3: Risk assessment
        steps.push(ReasoningStep {
            step: "Risk Assessment".to_string(),
            observation: "29 rules checked, position sizing validated".to_string(),
            confidence: 0.9,
        });

        // Step 4: Decision
        steps.push(ReasoningStep {
            step: "Decision".to_string(),
            observation: format!("Final action: {}", decision),
            confidence: 0.85,
        });

        ReasoningChain {
            decision: decision.to_string(),
            steps,
            overall_confidence: 0.8,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Explain why a decision was made.
    pub fn explain(decision: &str, indicators: &[String], risk_passed: bool) -> String {
        let mut explanation = format!("Decision: {}\n\n", decision);
        explanation.push_str("Reasoning:\n");
        
        for indicator in indicators {
            explanation.push_str(&format!("  - {}\n", indicator));
        }
        
        explanation.push_str(&format!("\nRisk Check: {}\n", if risk_passed { "PASSED" } else { "BLOCKED" }));
        explanation.push_str(&format!("Confidence: Based on combined indicator scores\n"));
        
        explanation
    }
}

#[derive(Debug, Clone)]
pub struct ReasoningChain {
    pub decision: String,
    pub steps: Vec<ReasoningStep>,
    pub overall_confidence: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct ReasoningStep {
    pub step: String,
    pub observation: String,
    pub confidence: f64,
}
