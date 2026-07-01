use std::collections::HashMap;
use crate::error::ReasoningError;

/// Self-Consistency: run N reasoning paths, vote by majority.
///
/// The simplest and cheapest chain to add. Run existing analysis
/// through different heuristics/orderings, only act if consensus.
pub struct SelfConsistency {
    /// Minimum number of paths that must agree.
    pub threshold: usize,
}

impl SelfConsistency {
    pub fn new(threshold: usize) -> Self {
        Self { threshold }
    }

    /// Run multiple reasoning paths and find consensus.
    pub fn vote<I: Clone, V: Eq + std::hash::Hash + Clone + std::fmt::Debug>(
        &self,
        input: &I,
        paths: &[Box<dyn Fn(&I) -> V + Send + Sync>],
    ) -> Result<VoteResult<V>, ReasoningError> {
        let mut votes: HashMap<V, usize> = HashMap::new();
        let mut results = Vec::new();

        for (i, path) in paths.iter().enumerate() {
            let result = path(input);
            *votes.entry(result.clone()).or_insert(0) += 1;
            results.push((i, result));
        }

        let total = paths.len();
        let winner = votes.iter()
            .max_by_key(|(_, count)| *count)
            .map(|(val, count)| (val.clone(), *count));

        match winner {
            Some((val, count)) if count >= self.threshold => Ok(VoteResult::Consensus {
                value: val,
                votes: count,
                total_paths: total,
            }),
            Some((val, count)) => Ok(VoteResult::NoConsensus {
                winner: val,
                votes: count,
                total_paths: total,
                threshold: self.threshold,
                all_votes: votes.into_iter().map(|(v, c)| (c, v)).collect(),
            }),
            None => Err(ReasoningError::ConsensusFailed {
                votes: Vec::new(),
                threshold: self.threshold,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub enum VoteResult<V: std::fmt::Debug> {
    Consensus { value: V, votes: usize, total_paths: usize },
    NoConsensus { winner: V, votes: usize, total_paths: usize, threshold: usize, all_votes: Vec<(usize, V)> },
}

impl<V: std::fmt::Debug> VoteResult<V> {
    pub fn is_consensus(&self) -> bool {
        matches!(self, VoteResult::Consensus { .. })
    }

    pub fn confidence(&self) -> f64 {
        match self {
            VoteResult::Consensus { votes, total_paths, .. } => *votes as f64 / *total_paths as f64,
            VoteResult::NoConsensus { votes, total_paths, .. } => *votes as f64 / *total_paths as f64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SignalDirection {
    Buy,
    Sell,
    Hold,
}

/// Trading: vote on signal direction from multiple analysis methods.
pub fn trading_vote(
    rsi_signal: &SignalDirection,
    macd_signal: &SignalDirection,
    volume_signal: &SignalDirection,
    trend_signal: &SignalDirection,
    pattern_signal: &SignalDirection,
) -> VoteResult<SignalDirection> {
    let sc = SelfConsistency::new(3); // need 3/5 agreement
    let inputs = (
        rsi_signal.clone(),
        macd_signal.clone(),
        volume_signal.clone(),
        trend_signal.clone(),
        pattern_signal.clone(),
    );

    let paths: Vec<Box<dyn Fn(&(SignalDirection, SignalDirection, SignalDirection, SignalDirection, SignalDirection)) -> SignalDirection + Send + Sync>> = vec![
        Box::new(|i| i.0.clone()),  // RSI view
        Box::new(|i| i.1.clone()),  // MACD view
        Box::new(|i| i.2.clone()),  // Volume view
        Box::new(|i| i.3.clone()),  // Trend view
        Box::new(|i| i.4.clone()),  // Pattern view
    ];

    sc.vote(&inputs, &paths).unwrap_or(VoteResult::NoConsensus {
        winner: SignalDirection::Hold,
        votes: 0,
        total_paths: 5,
        threshold: 3,
        all_votes: Vec::new(),
    })
}
