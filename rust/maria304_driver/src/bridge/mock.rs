//! In-memory mock bridge for integration tests.
//!
//! Captures every submitted [`CanonicalCommand`] in an ordered log
//! so tests can assert on the full envelope shape.  Optionally emits
//! a canned [`CanonicalResponse`] (default: ACK with `fiscal_id`
//! `"0000000001"` incrementing per submit).

use std::cell::RefCell;

use super::{Bridge, BridgeError, CanonicalCommand, CanonicalResponse};

/// In-memory bridge.  Not thread-safe — tests run single-threaded.
#[derive(Debug, Default)]
pub struct MockBridge {
    submitted: RefCell<Vec<CanonicalCommand>>,
    next_fiscal_id: RefCell<u64>,
    /// If set, every submit returns this error instead of a canned ACK.
    forced_error: RefCell<Option<BridgeError>>,
}

impl MockBridge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            submitted: RefCell::default(),
            next_fiscal_id: RefCell::new(1),
            forced_error: RefCell::default(),
        }
    }

    /// Force every subsequent submit to fail with the given error.
    pub fn force_error(&self, err: BridgeError) {
        *self.forced_error.borrow_mut() = Some(err);
    }

    /// Snapshot of all envelopes submitted so far.
    #[must_use]
    pub fn submitted(&self) -> Vec<CanonicalCommand> {
        self.submitted.borrow().clone()
    }

    /// Count of submits — convenience for assertions.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.submitted.borrow().len()
    }

    /// Last submitted envelope, if any.
    #[must_use]
    pub fn last(&self) -> Option<CanonicalCommand> {
        self.submitted.borrow().last().cloned()
    }
}

impl Bridge for MockBridge {
    fn submit(&self, command: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
        if let Some(err) = self.forced_error.borrow_mut().take() {
            return Err(err);
        }
        self.submitted.borrow_mut().push(command.clone());
        let mut n = self.next_fiscal_id.borrow_mut();
        let fiscal_id = format!("{:010}", *n);
        *n += 1;

        let (sale, ret) = (
            command.payload.totals.sale_kopecks,
            command.payload.totals.return_kopecks,
        );
        Ok(CanonicalResponse {
            ok: true,
            document_id: format!("doc-{fiscal_id}"),
            fiscal_id,
            fiscal_ts: "2026-04-20T10:00:00+00:00".to_string(),
            document_state: "ACK".to_string(),
            sale_total_kopecks: sale,
            return_total_kopecks: ret,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::dto::{CanonicalCommand, CommandType, ReceiptPayload};

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
    fn fresh_mock_has_no_submissions_and_assigns_fiscal_id_starting_at_one() {
        let b = MockBridge::new();
        assert_eq!(b.call_count(), 0);
        let r = b.submit(&sample_cmd()).unwrap();
        assert_eq!(r.fiscal_id, "0000000001");
        assert_eq!(b.call_count(), 1);
    }

    #[test]
    fn fiscal_id_increments_per_submission() {
        let b = MockBridge::new();
        let r1 = b.submit(&sample_cmd()).unwrap();
        let r2 = b.submit(&sample_cmd()).unwrap();
        let r3 = b.submit(&sample_cmd()).unwrap();
        assert_eq!(r1.fiscal_id, "0000000001");
        assert_eq!(r2.fiscal_id, "0000000002");
        assert_eq!(r3.fiscal_id, "0000000003");
    }

    #[test]
    fn submitted_log_preserves_insertion_order() {
        let b = MockBridge::new();
        let mut a = sample_cmd();
        a.idempotency_key = "A".to_string();
        let mut c = sample_cmd();
        c.idempotency_key = "C".to_string();
        let mut bb = sample_cmd();
        bb.idempotency_key = "B".to_string();
        b.submit(&a).unwrap();
        b.submit(&bb).unwrap();
        b.submit(&c).unwrap();
        let keys: Vec<String> = b
            .submitted()
            .into_iter()
            .map(|e| e.idempotency_key)
            .collect();
        assert_eq!(keys, vec!["A", "B", "C"]);
    }

    #[test]
    fn forced_error_consumes_once_then_resets() {
        let b = MockBridge::new();
        b.force_error(BridgeError::Transport("boom".to_string()));
        let err = b.submit(&sample_cmd()).unwrap_err();
        assert!(matches!(err, BridgeError::Transport(_)));
        assert_eq!(b.call_count(), 0, "failed submit must not be logged");
        // Next call succeeds.
        let ok = b.submit(&sample_cmd()).unwrap();
        assert_eq!(ok.fiscal_id, "0000000001");
    }

    #[test]
    fn mock_response_echoes_totals() {
        let b = MockBridge::new();
        let mut cmd = sample_cmd();
        cmd.payload.totals.sale_kopecks = 12345;
        cmd.payload.totals.return_kopecks = 678;
        let r = b.submit(&cmd).unwrap();
        assert_eq!(r.sale_total_kopecks, 12345);
        assert_eq!(r.return_total_kopecks, 678);
    }

    #[test]
    fn last_returns_none_before_any_submit_then_latest_after() {
        let b = MockBridge::new();
        assert!(b.last().is_none());
        b.submit(&sample_cmd()).unwrap();
        let mut second = sample_cmd();
        second.idempotency_key = "second".to_string();
        b.submit(&second).unwrap();
        assert_eq!(b.last().unwrap().idempotency_key, "second");
    }
}
