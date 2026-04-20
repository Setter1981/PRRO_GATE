//! Tracing subscriber initialisation helpers.
//!
//! Applications that don't already have a `tracing-subscriber`
//! configured can call [`init_json_subscriber`] once at startup to
//! get structured JSON logs on stderr.  The env var `MARIA304_LOG`
//! controls the level (default: `info`).
//!
//! Callers with their own subscriber should NOT call this — tracing
//! subscriber installation is process-global.

use tracing_subscriber::{fmt, EnvFilter};

/// Install a JSON tracing subscriber.  Idempotent — returns `Err`
/// silently if a subscriber has already been set.
///
/// # Errors
/// [`tracing::dispatcher::SetGlobalDefaultError`] if a global
/// subscriber is already installed; caller typically ignores this.
pub fn init_json_subscriber() -> Result<(), tracing::dispatcher::SetGlobalDefaultError> {
    let filter = EnvFilter::try_from_env("MARIA304_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(filter)
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cannot fully test subscriber installation in unit-test scope
    // (global state), but we can at least prove the builder chain
    // compiles and returns the expected Result type.
    #[test]
    fn init_is_callable_but_may_return_err_if_already_set() {
        let _ = init_json_subscriber();
        // A second call is guaranteed to Err because the first set
        // the global default.  We don't assert either outcome so the
        // test works even when run alongside other tests that
        // install their own subscribers.
    }
}
