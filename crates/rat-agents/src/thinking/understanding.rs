//! Understanding System — Market comprehension and pattern recognition.

pub struct UnderstandingSystem;

impl UnderstandingSystem {
    /// Understand current market conditions.
    pub fn understand_market(context: &MarketContext) -> MarketUnderstanding {
        let mut insights = Vec::new();

        // Trend analysis
        if context.trend > 0.6 {
            insights.push(MarketInsight {
                category: "Trend".to_string(),
                observation: "Strong bullish trend detected".to_string(),
                confidence: context.trend,
                implication: "Favorable for long positions".to_string(),
            });
        } else if context.trend < 0.4 {
            insights.push(MarketInsight {
                category: "Trend".to_string(),
                observation: "Strong bearish trend detected".to_string(),
                confidence: 1.0 - context.trend,
                implication: "Favorable for short positions".to_string(),
            });
        }

        // Volatility analysis
        if context.volatility > 0.7 {
            insights.push(MarketInsight {
                category: "Volatility".to_string(),
                observation: "High volatility environment".to_string(),
                confidence: context.volatility,
                implication: "Wider stops, smaller positions".to_string(),
            });
        }

        // Volume analysis
        if context.volume_ratio > 1.5 {
            insights.push(MarketInsight {
                category: "Volume".to_string(),
                observation: "Above-average volume".to_string(),
                confidence: 0.7,
                implication: "Strong conviction behind price move".to_string(),
            });
        }

        let overall_confidence = if insights.is_empty() {
            0.5
        } else {
            insights.iter().map(|i| i.confidence).sum::<f64>() / insights.len() as f64
        };

        MarketUnderstanding {
            symbol: context.symbol.clone(),
            insights,
            overall_confidence,
            recommendation: Self::generate_recommendation(&context),
        }
    }

    fn generate_recommendation(context: &MarketContext) -> String {
        if context.trend > 0.6 && context.volume_ratio > 1.2 {
            "Strong bullish setup — consider long entry".to_string()
        } else if context.trend < 0.4 && context.volume_ratio > 1.2 {
            "Strong bearish setup — consider short entry".to_string()
        } else if context.volatility > 0.7 {
            "High volatility — wait for clearer signals".to_string()
        } else {
            "Neutral conditions — no clear edge".to_string()
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarketContext {
    pub symbol: String,
    pub trend: f64,
    pub volatility: f64,
    pub volume_ratio: f64,
    pub rsi: f64,
    pub macd: f64,
}

#[derive(Debug, Clone)]
pub struct MarketUnderstanding {
    pub symbol: String,
    pub insights: Vec<MarketInsight>,
    pub overall_confidence: f64,
    pub recommendation: String,
}

#[derive(Debug, Clone)]
pub struct MarketInsight {
    pub category: String,
    pub observation: String,
    pub confidence: f64,
    pub implication: String,
}
