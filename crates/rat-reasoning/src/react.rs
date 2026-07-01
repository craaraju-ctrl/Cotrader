use crate::error::ReasoningError;

/// A trace entry for ReAct loops.
#[derive(Debug, Clone)]
pub struct ReactTrace<B: Clone, A: Clone, O: Clone> {
    pub belief: B,
    pub action: A,
    pub observation: O,
    pub step: usize,
}

/// ReAct agent — interleaves reasoning with real-world actions.
///
/// Pattern: observe → reason → act → observe → reason → act → ...
/// In trading: compute signal → query orderbook → check fill → re-compute.
pub trait ReactAgent: Send + Sync {
    type Belief: Clone + Send;
    type Action: Clone + Send;
    type Observation: Clone + Send;

    /// Update belief state based on observation history.
    fn reason(&self, belief: &Self::Belief, history: &[ReactTrace<Self::Belief, Self::Action, Self::Observation>]) -> Result<Self::Belief, ReasoningError>;

    /// Decide what action to take given current belief.
    fn act(&self, belief: &Self::Belief) -> Result<Self::Action, ReasoningError>;

    /// Query the world and get an observation.
    fn observe(&self, action: &Self::Action) -> Result<Self::Observation, ReasoningError>;

    /// Should we stop iterating?
    fn is_done(&self, belief: &Self::Belief, obs: &Self::Observation) -> bool;
}

/// Execute a ReAct loop — reason, act, observe, repeat until done or max_steps.
pub fn react_loop<A: ReactAgent>(
    agent: &A,
    initial_belief: A::Belief,
    max_steps: usize,
) -> Result<Vec<ReactTrace<A::Belief, A::Action, A::Observation>>, ReasoningError> {
    let mut belief = initial_belief;
    let mut history = Vec::new();

    for step in 0..max_steps {
        let action = agent.act(&belief)?;
        let obs = agent.observe(&action)?;

        if agent.is_done(&belief, &obs) {
            history.push(ReactTrace { belief, action, observation: obs, step });
            break;
        }

        let next_belief = agent.reason(&belief, &history)?;
        history.push(ReactTrace { belief: belief.clone(), action, observation: obs, step });
        belief = next_belief;
    }

    Ok(history)
}

/// Trading-specific ReAct implementation for signal generation.
pub struct TradingReactAgent {
    pub min_confidence: f64,
    pub max_iterations: usize,
}

impl ReactAgent for TradingReactAgent {
    type Belief = SignalBelief;
    type Action = MarketQuery;
    type Observation = MarketData;

    fn reason(&self, belief: &SignalBelief, history: &[ReactTrace<SignalBelief, MarketQuery, MarketData>]) -> Result<SignalBelief, ReasoningError> {
        // Update signal based on accumulated observations
        let mut updated = belief.clone();
        for trace in history {
            // Incorporate new data into belief
            updated.data_points += 1;
            if trace.observation.price_change > 0.0 {
                updated.bullish_evidence += 1;
            } else {
                updated.bearish_evidence += 1;
            }
        }
        // Recompute confidence
        let total = updated.bullish_evidence + updated.bearish_evidence;
        updated.confidence = if total > 0 {
            (updated.bullish_evidence as f64 / total as f64 - 0.5).abs() * 2.0
        } else {
            0.0
        };
        Ok(updated)
    }

    fn act(&self, belief: &SignalBelief) -> Result<MarketQuery, ReasoningError> {
        Ok(MarketQuery {
            symbol: belief.symbol.clone(),
            timeframe: belief.timeframe.clone(),
        })
    }

    fn observe(&self, _action: &MarketQuery) -> Result<MarketData, ReasoningError> {
        // In production, this queries the actual market data feed
        Ok(MarketData { price: 0.0, volume: 0.0, price_change: 0.0 })
    }

    fn is_done(&self, belief: &SignalBelief, _obs: &MarketData) -> bool {
        belief.confidence >= self.min_confidence || belief.data_points >= self.max_iterations
    }
}

#[derive(Debug, Clone)]
pub struct SignalBelief {
    pub symbol: String,
    pub timeframe: String,
    pub bullish_evidence: usize,
    pub bearish_evidence: usize,
    pub data_points: usize,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct MarketQuery {
    pub symbol: String,
    pub timeframe: String,
}

#[derive(Debug, Clone)]
pub struct MarketData {
    pub price: f64,
    pub volume: f64,
    pub price_change: f64,
}
