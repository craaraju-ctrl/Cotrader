//! # Resilience Module
//!
//! Provides reusable resilience components:
//! - `CircuitBreaker` — Prevents cascading failures
//! - `ResilientClient` — Combines Circuit Breaker + Retry for HTTP calls
//!
//! These components are designed to be used across the crate and by external consumers.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};

/// Circuit Breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// A reusable, thread-safe Circuit Breaker implementation.
///
/// FIXED (fault-isolation audit, D1): the previous implementation used three
/// separate mutexes with inconsistent acquisition order (`on_success`:
/// state → failure_count; `on_failure`: failure_count → … → state), a
/// confirmed ABBA deadlock under concurrent load (4/4 threads permanently
/// stuck in an 8M-op stress test). All state now lives behind ONE mutex with
/// tiny critical sections, so a deadlock is structurally impossible and the
/// sync lock never parks a Tokio worker beyond nanoseconds.
#[derive(Debug)]
struct BreakerInner {
    state: CircuitState,
    failure_count: u32,
    last_failure_time: Option<Instant>,
}

pub struct CircuitBreaker {
    inner: Mutex<BreakerInner>,
    failure_threshold: u32,
    reset_timeout: Duration,
    /// Optional hook fired exactly once per Closed/HalfOpen → Open transition
    /// (used by consumers to propagate the halt to other subsystems).
    on_open: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("inner", &self.inner)
            .field("failure_threshold", &self.failure_threshold)
            .field("reset_timeout", &self.reset_timeout)
            .finish()
    }
}

impl CircuitBreaker {
    /// Create a new Circuit Breaker.
    ///
    /// - `failure_threshold`: Number of consecutive failures before opening the circuit.
    /// - `reset_timeout_secs`: Time in seconds after which to attempt recovery (Half-Open).
    pub fn new(failure_threshold: u32, reset_timeout_secs: u64) -> Self {
        Self {
            inner: Mutex::new(BreakerInner {
                state: CircuitState::Closed,
                failure_count: 0,
                last_failure_time: None,
            }),
            failure_threshold,
            reset_timeout: Duration::from_secs(reset_timeout_secs),
            on_open: Mutex::new(None),
        }
    }

    /// Register a callback fired on every Closed/HalfOpen → Open transition.
    /// Callers use this to announce the halt on a global coordination channel.
    pub fn set_on_open(&self, cb: impl Fn() + Send + Sync + 'static) {
        *self.on_open.lock().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(cb));
    }

    /// Check if a request is allowed to proceed.
    pub fn can_execute(&self) -> bool {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match g.state {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => {
                if g
                    .last_failure_time
                    .map_or(false, |t| t.elapsed() >= self.reset_timeout)
                {
                    g.state = CircuitState::HalfOpen;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful request.
    pub fn on_success(&self) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.state = CircuitState::Closed;
        g.failure_count = 0;
    }

    /// Record a failed request.
    pub fn on_failure(&self) {
        let opened = {
            let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            g.failure_count += 1;
            g.last_failure_time = Some(Instant::now());
            if g.failure_count >= self.failure_threshold && g.state != CircuitState::Open {
                g.state = CircuitState::Open;
                true
            } else {
                false
            }
        }; // inner lock released BEFORE the callback — no nested locking
        if opened {
            if let Some(cb) = self
                .on_open
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
            {
                cb();
            }
        }
    }

    /// Get the current state of the circuit breaker.
    pub fn get_state(&self) -> CircuitState {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).state
    }
}

/// A reusable resilient HTTP client that combines Circuit Breaker + Retry.
pub struct ResilientClient {
    client: ClientWithMiddleware,
    circuit_breaker: CircuitBreaker,
}

impl ResilientClient {
    /// Create a new Resilient HTTP Client.
    pub fn new(max_retries: u32, failure_threshold: u32, reset_timeout_secs: u64) -> Self {
        let reqwest_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build reqwest client");

        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(max_retries);

        let client = ClientBuilder::new(reqwest_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        let circuit_breaker = CircuitBreaker::new(failure_threshold, reset_timeout_secs);

        Self {
            client,
            circuit_breaker,
        }
    }

    /// Access the inner circuit breaker (e.g. to register an `on_open` hook
    /// that propagates the halt to a global coordination channel).
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    /// Perform a resilient POST request with JSON body and custom headers.
    pub async fn post_json_with_headers<T, R>(
        &self,
        url: &str,
        body: &T,
        headers: &[(&str, &str)],
    ) -> Result<R, String>
    where
        T: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        if !self.circuit_breaker.can_execute() {
            return Err("Circuit breaker is OPEN. Service temporarily unavailable.".to_string());
        }

        let mut builder = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(body).unwrap_or_default());

        for (key, value) in headers {
            builder = builder.header(*key, *value);
        }

        let result = builder.send().await;

        match result {
            Ok(response) if response.status().is_success() => {
                self.circuit_breaker.on_success();
                response.json::<R>().await.map_err(|e| e.to_string())
            }
            Ok(response) => {
                self.circuit_breaker.on_failure();
                Err(format!("HTTP error: {}", response.status()))
            }
            Err(e) => {
                self.circuit_breaker.on_failure();
                Err(e.to_string())
            }
        }
    }

    /// Perform a resilient POST request with JSON body.
    pub async fn post_json<T, R>(&self, url: &str, body: &T) -> Result<R, String>
    where
        T: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        if !self.circuit_breaker.can_execute() {
            return Err("Circuit breaker is OPEN. Service temporarily unavailable.".to_string());
        }

        let result = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(body).unwrap_or_default())
            .send()
            .await;

        match result {
            Ok(response) if response.status().is_success() => {
                self.circuit_breaker.on_success();
                response.json::<R>().await.map_err(|e| e.to_string())
            }
            Ok(response) => {
                self.circuit_breaker.on_failure();
                Err(format!("HTTP error: {}", response.status()))
            }
            Err(e) => {
                self.circuit_breaker.on_failure();
                Err(e.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn opens_after_threshold_and_half_opens() {
        let cb = CircuitBreaker::new(3, 0);
        assert_eq!(cb.get_state(), CircuitState::Closed);
        for _ in 0..3 {
            cb.on_failure();
        }
        assert_eq!(cb.get_state(), CircuitState::Open);
        // reset_timeout = 0 → next can_execute() probes HalfOpen
        assert!(cb.can_execute());
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);
        cb.on_success();
        assert_eq!(cb.get_state(), CircuitState::Closed);
    }

    /// Regression test for the ABBA deadlock (D1): 4 threads × 2M mixed
    /// success/failure ops. With the old triple-mutex layout this deadlocked
    /// permanently; the single-mutex version must complete.
    #[test]
    fn no_deadlock_under_concurrent_hammer() {
        let cb = Arc::new(CircuitBreaker::new(1, 3600));
        let mut handles = Vec::new();
        for t in 0..4u64 {
            let cb = cb.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..2_000_000u64 {
                    if (t + i) % 2 == 0 {
                        cb.on_success();
                    } else {
                        cb.on_failure();
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn on_open_hook_fires_once_per_transition() {
        let cb = CircuitBreaker::new(2, 3600);
        let hits = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let h = hits.clone();
        cb.set_on_open(move || {
            h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        cb.on_failure();
        cb.on_failure(); // opens here
        cb.on_failure(); // already open — no second fire
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
