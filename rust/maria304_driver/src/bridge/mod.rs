//! Python PRRO Gateway bridge — trait + canonical envelope DTOs.
//!
//! The driver owns no fiscal business logic (invariant DRV-1).
//! Every receipt-close (`COMP`) produces a [`dto::CanonicalCommand`]
//! envelope, which a [`Bridge`] implementation forwards to the Python
//! gateway over HTTP.  The gateway is the single source of truth for
//! canonical data, DPS submission, and archival.
//!
//! Sprint M4 landed the trait + DTOs + an in-memory mock for tests.
//! Sprint M7 adds the real blocking reqwest client; the Python-side
//! `/v1/ingress/maria304` endpoint + `maria304_native.py` adapter
//! land alongside.

pub mod dto;
pub mod http_client;
pub mod mock;
pub mod retry;

pub use dto::{
    AcquirerSlip, CanonicalCommand, CanonicalPayment, CanonicalResponse, CommandType,
    DualTaxMode, FiscalLine, PaymentKind, RawFrame, ReceiptDirection, ReceiptPayload, Totals,
};
pub use http_client::HttpBridge;
pub use mock::MockBridge;
pub use retry::{RetryBridge, RetryPolicy, Sleeper, ThreadSleeper};

/// Errors surfaced by a [`Bridge`] implementation.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// The bridge refused the canonical envelope (e.g. the Python
    /// gateway validated it and returned a typed fiscal error).  Maps
    /// to a specific `SOFT*` wire code.
    #[error("bridge rejected: {code}")]
    Rejected { code: String, message: String },
    /// Transport layer failed (TCP, HTTP, timeout).  Caller should
    /// consider the request retryable unless the bridge explicitly
    /// says otherwise.
    #[error("bridge unreachable: {0}")]
    Transport(String),
}

/// Abstract bridge — the session dispatcher calls this on COMP.
///
/// All methods are synchronous for M4–M6.  M7 wraps an async reqwest
/// client behind a blocking facade so the call site in `dispatch`
/// does not need to become `async`.
pub trait Bridge {
    /// Submit a canonical envelope, receive a typed response.
    ///
    /// # Errors
    /// Any [`BridgeError`] — dispatcher maps to `SOFT*` wire code.
    fn submit(&self, command: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError>;
}
