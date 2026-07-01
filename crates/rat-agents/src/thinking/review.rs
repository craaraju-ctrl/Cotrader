//! Decision Review — Post-decision analysis and learning.

pub struct DecisionReview;

impl DecisionReview {
    /// Review a completed trade and extract lessons.
    pub fn review_trade(trade: &TradeRecord) -> ReviewResult {
        let mut lessons = Vec::new();

        // Analyze entry timing
        if trade.entry_price > trade.exit_price && trade.direction == "BUY" {
            lessons.push("Entered at a high point — consider waiting for pullback".to_string());
        }

        // Analyze exit timing
        if trade.exit_price > trade.entry_price && trade.direction == "SELL" {
            lessons.push("Exited at a low point — could have held longer".to_string());
        }

        // Analyze risk management
        if trade.pnl < 0.0 && trade.holding_time < 60 {
            lessons.push("Quick loss — review stop-loss placement".to_string());
        }

        // Analyze position sizing
        if trade.position_size > 0.02 {
            lessons.push("Large position size — consider reducing risk".to_string());
        }

        ReviewResult {
            trade_id: trade.id.clone(),
            pnl: trade.pnl,
            lessons,
            score: Self::calculate_score(trade),
            timestamp: chrono::Utc::now(),
        }
    }

    fn calculate_score(trade: &TradeRecord) -> f64 {
        let mut score: f64 = 0.5; // Base score

        // Reward profitable trades
        if trade.pnl > 0.0 {
            score += 0.3;
        }

        // Reward good risk/reward ratio
        if trade.risk_reward > 2.0 {
            score += 0.1;
        }

        // Penalize oversized positions
        if trade.position_size > 0.02 {
            score -= 0.1;
        }

        score.clamp(0.0_f64, 1.0)
    }
}

#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub id: String,
    pub symbol: String,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub pnl: f64,
    pub position_size: f64,
    pub risk_reward: f64,
    pub holding_time: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub trade_id: String,
    pub pnl: f64,
    pub lessons: Vec<String>,
    pub score: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
