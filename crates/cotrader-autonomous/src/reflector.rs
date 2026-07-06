use crate::state::SharedState;
use async_trait::async_trait;
use chrono::Utc;
use std::error::Error;
use cotrader_core::{
    Agent, AgentInput, AgentOutput, AgentTier, PostTradeReflection, TradingEpisode,
};

pub struct ReflectorAgent {
    pub state: SharedState,
}

impl ReflectorAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Lightweight daily reflection — reads today's portfolio state.
    pub async fn reflect(&self, symbol: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        println!("[Reflector] Reflecting on past decisions for {}...", symbol);

        let today_key = format!("decisions/{}/{}", symbol, Utc::now().format("%Y%m%d"));
        let _recent = self.state.agent_memory.memory.get_decision(&today_key).ok().flatten();

        let pattern_key = format!("patterns/{}", symbol);
        let _patterns = self.state.agent_memory.memory.get_decision(&pattern_key).ok().flatten();

        let portfolio = self.state.portfolio_store.portfolio.read().await;

        // Get open position for this symbol if any
        let position_info = portfolio.open_positions.iter()
            .find(|p| p.symbol == symbol)
            .map(|p| {
                let pnl_pct = if p.entry_price > 0.0 {
                    (p.current_price - p.entry_price) / p.entry_price * 100.0
                } else { 0.0 };
                format!("Open {} {:.2} shares @ {:.2} (current {:.2}, P&L {:+.2}%)",
                    if p.direction == cotrader_core::TradeDirection::Long { "LONG" } else { "SHORT" },
                    p.quantity, p.entry_price, p.current_price, pnl_pct)
            })
            .unwrap_or_else(|| "No open position".to_string());

        let reflection = format!(
            "Reflection for {}: Daily P&L: ${:.2} | Trades: {} | Wins: {} | Losses: {} | Consecutive Losses: {} | Position: {}",
            symbol, portfolio.daily_pnl, portfolio.total_trades_today,
            portfolio.winning_trades_today, portfolio.losing_trades_today,
            portfolio.consecutive_losses, position_info
        );

        println!("[Reflector] {}", reflection);

        let reflection_key = format!("reflections/{}/{}", symbol, Utc::now().timestamp());
        let _ = self
            .state
            .agent_memory.memory
            .store_decision(&reflection_key, &reflection);

        Ok(reflection)
    }

    /// Deep post-trade reflection — computes a structured PostTradeReflection
    /// deterministically from episode data (no LLM dependency).
    /// The LLM-based reflection has been removed since the LLM is now stubbed.
    /// Regret score is computed from outcome P&L: higher loss = higher regret.
    pub async fn deep_reflect_on_episode(
        &self,
        episode: &TradingEpisode,
    ) -> Result<PostTradeReflection, Box<dyn Error + Send + Sync>> {
        println!(
            "[Reflector] 🔬 Deep reflecting on episode {}...",
            episode.episode_id
        );

        // Compute regret from outcome: 0.0 = perfect, 1.0 = terrible mistake
        let (regret, lesson, should_alert) = match &episode.outcome {
            Some(o) => {
                let abs_pnl_pct = o.pnl_pct.abs();
                // Regret is proportional to how much we lost vs expected
                let regret = if o.pnl_pct < 0.0 {
                    (abs_pnl_pct * 5.0).clamp(0.0, 1.0) // losing trade
                } else {
                    (abs_pnl_pct * 0.5).clamp(0.0, 0.3) // winning trade: low regret
                };
                let lesson = match o.exit_reason.as_str() {
                    "stop_loss" => format!(
                        "Stop-loss hit at {:.2} for {} (P&L: {:+.2}%). SL was {:.2} → entry {:.2}.",
                        o.exit_price, episode.symbol, o.pnl_pct * 100.0,
                        episode.stop_loss, episode.entry_price
                    ),
                    "take_profit" => format!(
                        "Take-profit at {:.2} ({:+.2}%). Entry {:.2} → TP {:.2}.",
                        o.exit_price, o.pnl_pct * 100.0,
                        episode.entry_price, episode.take_profit
                    ),
                    _ => format!(
                        "{} closed at {:.2} with P&L {:+.2}%.",
                        episode.symbol, o.exit_price, o.pnl_pct * 100.0
                    ),
                };
                let should_alert = o.pnl_pct < -0.05; // >5% loss → alert
                (regret, lesson, should_alert)
            }
            None => (0.0, format!("Trade still open for {}.", episode.symbol), false),
        };

        self.state
            .push_live_comm(
                "Guardian",
                "Reflector",
                "REFLECT",
                &format!(
                    "Deterministic reflection for {} (Episode {}): regret={:.2}",
                    episode.symbol, episode.episode_id, regret
                ),
                Some(episode.symbol.clone()),
            )
            .await;

        // Build structured reflection
        let reflection = PostTradeReflection {
            timestamp: Utc::now(),
            lesson: lesson.clone(),
            violated_assumptions: Vec::new(),
            regret_score: regret,
            what_went_wrong: Vec::new(),
            what_went_right: Vec::new(),
            suggested_rule_change: None,
            should_alert,
        };

        // Store the reflection in memory alongside the episode
        if let Ok(json) = serde_json::to_string(&reflection) {
            let key = format!("reflection/{}", episode.episode_id);
            let _ = self.state.agent_memory.memory.store_state(&key, &json);
        }

        // Store a meaningful reflection summary for retrieval
        let reflection_summary = format!(
            "{} {} entry={:.2} exit={:.2} pnl={:+.2}% reason={} lesson={} regret={:.2}",
            episode.symbol, episode.action, episode.entry_price,
            episode.outcome.as_ref().map(|o| o.exit_price).unwrap_or(0.0),
            episode.outcome.as_ref().map(|o| o.pnl_pct * 100.0).unwrap_or(0.0),
            episode.outcome.as_ref().map(|o| o.exit_reason.as_str()).unwrap_or("open"),
            lesson, regret
        );
        let reflection_key = format!("reflections/{}/{}", episode.symbol, Utc::now().timestamp());
        let _ = self.state.agent_memory.memory.store_decision(&reflection_key, &reflection_summary);

        // Promote to vector memory for trained intelligence
        let summary = format!(
            "{} reflection: lesson={} regret={:.2}",
            episode.symbol, lesson, regret
        );
        // Store reflection in vector memory for future semantic recall
        let vm = self.state.agent_memory.vector_memory.clone();
        let eid = episode.episode_id.clone();
        let sym = episode.symbol.clone();
        tokio::spawn(async move {
            let mut vm_write = vm.write().await;
            if let Err(e) = vm_write.store(&eid, &sym, &summary, Some(regret)).await {
                eprintln!("[Reflector] ⚠ Failed to store reflection in vector memory: {}", e);
            }
        });

        if should_alert {
            println!("[Reflector] 🚨 CRITICAL LESSON: {}", lesson);
            cotrader_core::notifier::alert(
                "CRITICAL REFLECTION",
                &format!(
                    "{}: {} (regret {:.2})",
                    episode.symbol, lesson, regret
                ),
            )
            .await;
        }

        println!(
            "[Reflector] ✅ Deep reflection complete — regret: {:.2}, lesson: {}",
            regret, lesson
        );

        Ok(reflection)
    }
}

#[async_trait]
impl Agent for ReflectorAgent {
    fn name(&self) -> &str {
        "ReflectorAgent"
    }
    fn tier(&self) -> AgentTier {
        AgentTier::Main
    }

    async fn run(
        &self,
        input: Option<AgentInput>,
    ) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        let symbol = match &input {
            Some(AgentInput::ConfluenceRequest { context }) => context.symbol.clone(),
            _ => "NIFTY".to_string(),
        };

        let _ = self.reflect(&symbol).await;
        Ok(AgentOutput::Done)
    }
}
