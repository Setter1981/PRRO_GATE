//! Retry wrapper around an inner [`Bridge`].
//!
//! Transport-class failures (connection refused, timeout, 5xx) are
//! retried with exponential backoff.  Typed rejections (4xx with
//! `SOFTBADART` etc) are returned immediately — retrying an
//! operator-caused error would make the ACK take longer without
//! changing the outcome.
//!
//! The retry wrapper is a pure decorator — it does not cache, queue,
//! or persist anything.  Callers still get a final Err after the
//! retries are exhausted; the dispatcher maps that to SOFTBLOCK.

use std::thread;
use std::time::Duration;

use super::{Bridge, BridgeError, CanonicalCommand, CanonicalResponse};

/// Retry policy — pure data, no state.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Total attempts (initial + retries).  `1` disables retries.
    pub max_attempts: u32,
    /// Initial backoff delay before the first retry.
    pub initial_backoff: Duration,
    /// Multiplier between successive retries (e.g. 2.0 doubles).
    pub backoff_multiplier: f64,
    /// Hard cap on per-retry sleep.
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(250),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy {
    /// Delay before the `retry_number`-th retry (0-indexed — the
    /// first retry uses `initial_backoff`).
    #[must_use]
    pub fn backoff_for(&self, retry_number: u32) -> Duration {
        let base = self.initial_backoff.as_secs_f64();
        let exp = i32::try_from(retry_number).unwrap_or(i32::MAX);
        let mult = self.backoff_multiplier.powi(exp);
        let seconds = (base * mult).min(self.max_backoff.as_secs_f64());
        Duration::from_secs_f64(seconds.max(0.0))
    }
}

/// Injection point for the sleep primitive — tests swap this out for
/// a mock so they don't actually block.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, d: Duration);
}

/// Default blocking sleeper.
#[derive(Debug, Default)]
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, d: Duration) {
        thread::sleep(d);
    }
}

/// Wrap any `Bridge` with a retry policy.
pub struct RetryBridge<B: Bridge + Send + Sync> {
    inner: B,
    policy: RetryPolicy,
    sleeper: Box<dyn Sleeper>,
}

impl<B: Bridge + Send + Sync> RetryBridge<B> {
    #[must_use]
    pub fn new(inner: B, policy: RetryPolicy) -> Self {
        Self {
            inner,
            policy,
            sleeper: Box::new(ThreadSleeper),
        }
    }

    /// Plug in a custom sleeper — mainly for tests.
    #[must_use]
    pub fn with_sleeper(inner: B, policy: RetryPolicy, sleeper: Box<dyn Sleeper>) -> Self {
        Self {
            inner,
            policy,
            sleeper,
        }
    }
}

impl<B: Bridge + Send + Sync> Bridge for RetryBridge<B> {
    fn submit(&self, command: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
        let mut last_err: BridgeError = BridgeError::Transport("no attempts".to_string());
        let total = self.policy.max_attempts.max(1);
        for attempt in 0..total {
            match self.inner.submit(command) {
                Ok(r) => return Ok(r),
                Err(BridgeError::Rejected { code, message }) => {
                    // Typed rejection — do NOT retry.  The gateway
                    // already decided the envelope is bad; retrying
                    // would just delay the error surface.
                    return Err(BridgeError::Rejected { code, message });
                }
                Err(e @ BridgeError::Transport(_)) => {
                    last_err = e;
                    if attempt + 1 < total {
                        self.sleeper.sleep(self.policy.backoff_for(attempt));
                    }
                }
            }
        }
        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::dto::{CanonicalCommand, CommandType, ReceiptPayload};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSleeper {
        durations: Mutex<Vec<Duration>>,
    }
    impl Sleeper for RecordingSleeper {
        fn sleep(&self, d: Duration) {
            self.durations.lock().unwrap().push(d);
        }
    }

    #[derive(Default)]
    struct ScriptedBridge {
        script: Mutex<Vec<Result<CanonicalResponse, BridgeError>>>,
        calls: Mutex<u32>,
    }
    impl ScriptedBridge {
        fn new(script: Vec<Result<CanonicalResponse, BridgeError>>) -> Self {
            Self {
                script: Mutex::new(script),
                calls: Mutex::new(0),
            }
        }
    }
    impl Bridge for ScriptedBridge {
        fn submit(&self, _: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
            *self.calls.lock().unwrap() += 1;
            let mut s = self.script.lock().unwrap();
            if s.is_empty() {
                return Err(BridgeError::Transport("script exhausted".to_string()));
            }
            s.remove(0)
        }
    }

    fn sample_ok() -> CanonicalResponse {
        CanonicalResponse {
            ok: true,
            document_id: "d".to_string(),
            fiscal_id: "1".to_string(),
            fiscal_ts: "t".to_string(),
            document_state: "ACK".to_string(),
            sale_total_kopecks: 0,
            return_total_kopecks: 0,
        }
    }

    fn sample_cmd() -> CanonicalCommand {
        CanonicalCommand {
            schema_version: "1.0".to_string(),
            fiscal_number: "F".to_string(),
            command_type: CommandType::Sell,
            idempotency_key: "k".to_string(),
            cashier_id: None,
            department: None,
            return_check_number: None,
            payload: ReceiptPayload::default(),
        }
    }

    #[test]
    fn backoff_for_zero_returns_initial_delay() {
        let p = RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(10),
        };
        assert_eq!(p.backoff_for(0), Duration::from_millis(100));
    }

    #[test]
    fn backoff_multiplies_per_retry() {
        let p = RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(10),
        };
        assert_eq!(p.backoff_for(1), Duration::from_millis(200));
        assert_eq!(p.backoff_for(2), Duration::from_millis(400));
    }

    #[test]
    fn backoff_caps_at_max() {
        let p = RetryPolicy {
            max_attempts: 10,
            initial_backoff: Duration::from_secs(1),
            backoff_multiplier: 10.0,
            max_backoff: Duration::from_secs(3),
        };
        // 1s * 10^0 = 1  (below cap)
        assert_eq!(p.backoff_for(0), Duration::from_secs(1));
        // 1s * 10^1 = 10  → capped to 3
        assert_eq!(p.backoff_for(1), Duration::from_secs(3));
        // 1s * 10^2 = 100 → capped to 3
        assert_eq!(p.backoff_for(2), Duration::from_secs(3));
    }

    #[test]
    fn succeeds_on_first_try_without_sleeping() {
        let sleeper = Box::new(RecordingSleeper::default());
        let b = RetryBridge::with_sleeper(
            ScriptedBridge::new(vec![Ok(sample_ok())]),
            RetryPolicy::default(),
            sleeper,
        );
        let r = b.submit(&sample_cmd()).unwrap();
        assert_eq!(r.fiscal_id, "1");
    }

    #[test]
    fn retries_on_transport_then_succeeds() {
        let recording = Box::new(RecordingSleeper::default());
        let bridge = RetryBridge::with_sleeper(
            ScriptedBridge::new(vec![
                Err(BridgeError::Transport("timeout".to_string())),
                Err(BridgeError::Transport("reset".to_string())),
                Ok(sample_ok()),
            ]),
            RetryPolicy {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(10),
                backoff_multiplier: 2.0,
                max_backoff: Duration::from_secs(1),
            },
            recording,
        );
        let r = bridge.submit(&sample_cmd()).unwrap();
        assert_eq!(r.fiscal_id, "1");
    }

    #[test]
    fn rejected_is_not_retried_even_if_attempts_allow_it() {
        let recording = RecordingSleeper::default();
        let sleeper = Box::new(RecordingSleeper::default());
        let _ = recording; // type-checker silencer; not actually used
        let inner = ScriptedBridge::new(vec![Err(BridgeError::Rejected {
            code: "SOFTBADART".to_string(),
            message: "x".to_string(),
        })]);
        let bridge = RetryBridge::with_sleeper(inner, RetryPolicy::default(), sleeper);
        let err = bridge.submit(&sample_cmd()).unwrap_err();
        match err {
            BridgeError::Rejected { code, .. } => assert_eq!(code, "SOFTBADART"),
            BridgeError::Transport(_) => panic!("unexpected Transport"),
        }
    }

    #[test]
    fn exhausts_attempts_then_returns_last_transport_error() {
        let sleeper = Box::new(RecordingSleeper::default());
        let inner = ScriptedBridge::new(vec![
            Err(BridgeError::Transport("a".to_string())),
            Err(BridgeError::Transport("b".to_string())),
            Err(BridgeError::Transport("c".to_string())),
        ]);
        let bridge = RetryBridge::with_sleeper(
            inner,
            RetryPolicy {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(1),
                backoff_multiplier: 1.0,
                max_backoff: Duration::from_secs(1),
            },
            sleeper,
        );
        let err = bridge.submit(&sample_cmd()).unwrap_err();
        match err {
            BridgeError::Transport(msg) => assert_eq!(msg, "c"),
            BridgeError::Rejected { .. } => panic!("expected Transport"),
        }
    }

    #[test]
    fn max_attempts_one_means_no_retries() {
        let sleeper = Box::new(RecordingSleeper::default());
        let inner = ScriptedBridge::new(vec![
            Err(BridgeError::Transport("once".to_string())),
            Ok(sample_ok()), // never reached
        ]);
        let policy = RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        };
        let bridge = RetryBridge::with_sleeper(inner, policy, sleeper);
        assert!(matches!(
            bridge.submit(&sample_cmd()),
            Err(BridgeError::Transport(_))
        ));
    }
}
