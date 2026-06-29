use thiserror::Error;

/// Categorised errors that the Orchestra pipeline can encounter.
/// Each variant carries a recommended fallback strategy.
#[derive(Debug, Error)]
pub enum OrchestraError {
    /// Incoming data stream dropped or timed out
    #[error("Stream timeout for {symbol}: {detail}")]
    StreamTimeout {
        symbol: String,
        detail: String,
    },

    /// Invalid or malformed input data
    #[error("Malformed data for {symbol}: {detail}")]
    MalformedData {
        symbol: String,
        detail: String,
    },

    /// Analysis stage failed to produce a coherent signal
    #[error("Analysis failure on {symbol}: {detail}")]
    AnalysisFailure {
        symbol: String,
        detail: String,
    },

    /// Memory agent unreachable or returned error
    #[error("Memory agent error: {detail}")]
    MemoryAgentError {
        detail: String,
    },

    /// Decision was made but execution was blocked (risk, position limits)
    #[error("Execution blocked for {symbol}: {detail}")]
    ExecutionBlocked {
        symbol: String,
        detail: String,
    },

    /// Pipeline stage timeout (exceeded configured max processing time)
    #[error("Pipeline stage '{stage}' timed out after {elapsed_ms}ms")]
    StageTimeout {
        stage: String,
        elapsed_ms: u64,
    },

    /// Internal orchestrator error (shouldn't happen in normal operation)
    #[error("Orchestra internal error: {detail}")]
    Internal {
        detail: String,
    },
}

/// A fallback strategy determines how the pipeline should recover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackStrategy {
    /// Retry the operation up to N times with exponential backoff
    RetryWithBackoff,
    /// Skip this data point and move to the next
    SkipDataPoint,
    /// Enter a safe "hold" state — no new trades, monitor only
    SafeHold,
    /// Reset the pipeline stage and re-acquire state
    ResetStage,
    /// Use the last known good data point instead
    UseLastKnownGood,
    /// Emergency shutdown of this agent only
    ShutdownAgent,
}

impl OrchestraError {
    /// Return the recommended fallback strategy for this error type.
    pub fn fallback_strategy(&self) -> FallbackStrategy {
        match self {
            OrchestraError::StreamTimeout { .. } => FallbackStrategy::RetryWithBackoff,
            OrchestraError::MalformedData { .. } => FallbackStrategy::SkipDataPoint,
            OrchestraError::AnalysisFailure { .. } => FallbackStrategy::UseLastKnownGood,
            OrchestraError::MemoryAgentError { .. } => FallbackStrategy::SafeHold,
            OrchestraError::ExecutionBlocked { .. } => FallbackStrategy::SkipDataPoint,
            OrchestraError::StageTimeout { .. } => FallbackStrategy::RetryWithBackoff,
            OrchestraError::Internal { .. } => FallbackStrategy::ShutdownAgent,
        }
    }

    /// Return true if this error warrants alerting the operator.
    pub fn requires_alert(&self) -> bool {
        matches!(
            self,
            OrchestraError::MemoryAgentError { .. }
                | OrchestraError::Internal { .. }
                | OrchestraError::StreamTimeout { .. }
        )
    }

    /// Return the severity level for diagnostics.
    pub fn severity(&self) -> &'static str {
        match self {
            OrchestraError::StreamTimeout { .. } => "warn",
            OrchestraError::MalformedData { .. } => "warn",
            OrchestraError::AnalysisFailure { .. } => "info",
            OrchestraError::MemoryAgentError { .. } => "error",
            OrchestraError::ExecutionBlocked { .. } => "info",
            OrchestraError::StageTimeout { .. } => "warn",
            OrchestraError::Internal { .. } => "error",
        }
    }
}

/// Type alias for Orchestra results.
pub type OrchestraResult<T> = Result<T, OrchestraError>;

// ── Fallback Executor ─────────────────────────────────────

/// Execute a fallback strategy and return a recovery action description.
pub fn execute_fallback(strategy: FallbackStrategy, error: &OrchestraError) -> String {
    match strategy {
        FallbackStrategy::RetryWithBackoff => {
            tracing::warn!(
                "[Orchestra] RetryWithBackoff for: {}",
                error
            );
            "Scheduling retry with exponential backoff (2s, 4s, 8s max)".into()
        }
        FallbackStrategy::SkipDataPoint => {
            tracing::warn!(
                "[Orchestra] SkipDataPoint for: {}",
                error
            );
            "Skipping malformed/unprocessable data point".into()
        }
        FallbackStrategy::SafeHold => {
            tracing::warn!(
                "[Orchestra] SafeHold triggered by: {}",
                error
            );
            "Entering safe-hold mode — monitoring only, no trade execution".into()
        }
        FallbackStrategy::ResetStage => {
            tracing::warn!(
                "[Orchestra] ResetStage for: {}",
                error
            );
            "Resetting pipeline stage and re-acquiring state".into()
        }
        FallbackStrategy::UseLastKnownGood => {
            tracing::info!(
                "[Orchestra] UseLastKnownGood for: {}",
                error
            );
            "Using last known good data point as fallback".into()
        }
        FallbackStrategy::ShutdownAgent => {
            tracing::error!(
                "[Orchestra] ShutdownAgent for unrecoverable error: {}",
                error
            );
            "EMERGENCY: Agent shutdown initiated — manual intervention required".into()
        }
    }
}
