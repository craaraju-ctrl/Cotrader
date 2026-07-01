use crate::error::ReasoningError;
use crate::chain::State;

/// A reflection record — what was tried and what was learned.
#[derive(Debug, Clone)]
pub struct Reflection {
    pub attempt: usize,
    pub outcome: String,
    pub score: f64,
    pub lessons: Vec<String>,
}

/// Reflexion: execute → evaluate → reflect → retry with lessons.
///
/// Each attempt stores deterministic lessons (rule-based, not LLM).
/// Next attempt receives all prior reflections as context.
pub struct ReflexionLoop {
    pub max_attempts: usize,
}

impl ReflexionLoop {
    pub fn new(max_attempts: usize) -> Self {
        Self { max_attempts }
    }

    /// Run the reflexion loop with deterministic execution and reflection.
    pub fn run<F, R>(
        &self,
        initial: &State,
        execute: F,
        reflect: R,
    ) -> Result<(State, Vec<Reflection>), ReasoningError>
    where
        F: Fn(&State, &[Reflection]) -> Result<(State, f64), ReasoningError>,
        R: Fn(&State, f64) -> Vec<String>,
    {
        let mut state = initial.clone();
        let mut memory = Vec::new();

        for attempt in 0..self.max_attempts {
            let (output, score) = execute(&state, &memory)?;

            if score >= 0.9 {
                return Ok((output, memory));
            }

            let lessons = reflect(&output, score);
            memory.push(Reflection {
                attempt,
                outcome: format!("Score: {:.2}", score),
                score,
                lessons,
            });

            state = output;
        }

        Err(ReasoningError::MaxAttemptsExceeded { max: self.max_attempts })
    }
}

/// Deterministic reflection rules for trading strategies.
pub fn trading_reflect(state: &State, score: f64) -> Vec<String> {
    let mut lessons = Vec::new();

    let win_rate = state.get("win_rate").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let max_dd = state.get("max_drawdown").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let sharpe = state.get("sharpe").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let trade_count = state.get("trade_count").and_then(|v| v.as_f64()).unwrap_or(0.0);

    if win_rate < 0.45 {
        lessons.push("Win rate too low — tighten entry criteria or add confirmation filter".into());
    }
    if max_dd > 0.15 {
        lessons.push("Max drawdown exceeds 15% — reduce position sizing or add drawdown stop".into());
    }
    if sharpe < 1.0 {
        lessons.push("Sharpe below 1.0 — improve risk-adjusted returns via better exits".into());
    }
    if trade_count < 30.0 {
        lessons.push("Insufficient sample size — need 30+ trades for statistical significance".into());
    }
    if score < 0.5 {
        lessons.push("Overall score critically low — consider abandoning this strategy variant".into());
    }

    lessons
}

/// Apply reflections to modify strategy parameters.
pub fn apply_reflections(state: &mut State, reflections: &[Reflection]) {
    for reflection in reflections {
        for lesson in &reflection.lessons {
            if lesson.contains("reduce position sizing") {
                if let Some(size) = state.get_mut("position_size") {
                    if let Some(v) = size.as_f64() {
                        *size = serde_json::json!(v * 0.8);
                    }
                }
            }
            if lesson.contains("tighten entry") {
                if let Some(threshold) = state.get_mut("entry_threshold") {
                    if let Some(v) = threshold.as_f64() {
                        *threshold = serde_json::json!(v * 1.1);
                    }
                }
            }
            if lesson.contains("add confirmation") {
                let current_filters: Vec<String> = state.get("filters")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                if !current_filters.contains(&"volume_confirmation".to_string()) {
                    let mut new_filters: Vec<serde_json::Value> = current_filters.iter().map(|s| serde_json::json!(s)).collect();
                    new_filters.push(serde_json::json!("volume_confirmation"));
                    state.insert("filters".into(), serde_json::Value::Array(new_filters));
                }
            }
        }
    }
}
