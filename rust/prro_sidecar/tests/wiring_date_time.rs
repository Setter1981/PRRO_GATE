//! Regression guard — M7-Py-4a-1.
//!
//! The sidecar handler at `src/bin/prro_sidecar.rs` MUST derive
//! `Check.date_time` from `cmd.business_ts` via
//! `time_utils::kyiv_local_epoch`.  The previous implementation used
//! `now.unix_timestamp()` which silently diverged from the Python
//! transport path under any build-to-send delay.  This test enforces
//! the call-site stays wired to the helper rather than wall-clock.
//!
//! Because setting up the full async handler with crypto + repo +
//! grpc pool mocks is heavy, we take a targeted source-level approach:
//! read the handler source and assert the business_ts-derived helper
//! is present and `now.unix_timestamp()` is NOT used for `date_time`.
//!
//! ⚠ BRITTLENESS: this test matches literal substrings.  If
//! `prro_sidecar.rs` renames the `cmd` variable, changes whitespace
//! around the struct-field alignment, or restructures the Check build,
//! the literal matches will stop being accurate.  When such a
//! refactor lands, UPDATE THE LITERAL STRINGS BELOW, do not delete
//! the test — the invariant (date_time derived from business_ts, not
//! wall-clock) must remain enforced.  If this brittleness bites more
//! than once, consider parsing via `syn` crate instead of string
//! matching.

use std::fs;

#[test]
fn sidecar_check_date_time_comes_from_business_ts_not_now() {
    let src = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bin/prro_sidecar.rs"
    ))
    .expect("prro_sidecar.rs is readable");

    // Must call kyiv_local_epoch with cmd.business_ts.
    assert!(
        src.contains("kyiv_local_epoch(&cmd.business_ts)"),
        "expected Check.date_time to be derived via \
         kyiv_local_epoch(&cmd.business_ts); source must contain that call",
    );

    // Must NOT set date_time from now.unix_timestamp().  We look for
    // the literal combination of the field assignment and the
    // forbidden call in the same statement.
    let forbidden = src.contains("date_time:    now.unix_timestamp()")
        || src.contains("date_time: now.unix_timestamp()");
    assert!(
        !forbidden,
        "regression: Check.date_time must NOT be set from now.unix_timestamp(); \
         use time_utils::kyiv_local_epoch(&cmd.business_ts) instead",
    );
}


#[test]
fn sidecar_emits_pipeline_delay_metrics() {
    // M7-Py-4a-2: ops must be able to alert on abnormal gaps between
    // build/sign/send stages (TSP hang, gRPC pool contention, slow
    // business_ts drift).  Enforce the three tracing events exist.
    let src = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bin/prro_sidecar.rs"
    ))
    .expect("prro_sidecar.rs is readable");

    for needle in [
        "sidecar_build_sign_timed",
        "sidecar_grpc_send_done",
        "sidecar_sign_failed",
        "sidecar_grpc_send_failed",
        "business_ts_drift_ms",
        "business_ts_parse_failed_for_drift_metric",
        "build_ms",
        "sign_ms",
        "send_ms",
    ] {
        assert!(
            src.contains(needle),
            "expected sidecar to emit metric field/event {:?}; \
             removal would blind ops to pipeline-delay regressions",
            needle,
        );
    }
}


#[test]
fn xml_builder_format_ts_uses_iana_kyiv_tz_not_fixed_utc_plus_3() {
    // Source-level guard: xml_builder::format_ts must delegate to
    // time_utils::kyiv_local_yyyymmddhhmmss (which uses IANA
    // Europe/Kyiv with DST) — not the old hardcoded UTC+3 offset.
    let src = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/xml_builder.rs"
    ))
    .expect("xml_builder.rs is readable");

    assert!(
        src.contains("kyiv_local_yyyymmddhhmmss"),
        "expected xml_builder.rs to delegate timestamp formatting to \
         time_utils::kyiv_local_yyyymmddhhmmss",
    );

    assert!(
        !src.contains("UtcOffset::from_hms(3, 0, 0)"),
        "regression: xml_builder must not hardcode UTC+3 — that produces \
         a winter (Oct–Mar) divergence with the DST-aware \
         Check.date_time and with Python's zoneinfo-based path",
    );
}


// ── S2: envelope integrity ─────────────────────────────────────────────────────

#[cfg(test)]
mod envelope_integrity {
    use prro_sidecar::input::{CanonicalCommand, OperationType};
    use sha2::{Digest, Sha256};
    use serde_json::json;

    fn make_cmd(schema_version: &str, payload_sha256: &str) -> CanonicalCommand {
        serde_json::from_value(json!({
            "schema_version":  schema_version,
            "request_id":      "req-1",
            "idempotency_key": "idem-1",
            "operation_type":  "SELL",
            "fiscal_number":   "9999999999",
            "business_ts":     "2026-04-22T10:00:00+03:00",
            "payload_sha256":  payload_sha256,
            "payload":         {"receipt": {}}
        }))
        .expect("make_cmd: JSON must parse")
    }

    #[test]
    fn unknown_schema_version_rejected() {
        let cmd = make_cmd("2.0", "");
        let err = prro_sidecar::validate_envelope(&cmd).unwrap_err();
        assert!(err.contains("schema_version"), "got: {err}");
    }

    #[test]
    fn known_schema_version_passes() {
        let cmd = make_cmd("1.0", "");
        assert!(prro_sidecar::validate_envelope(&cmd).is_ok());
    }

    #[test]
    fn correct_payload_sha256_passes() {
        let payload = json!({"receipt": {}});
        let bytes = serde_json::to_vec(&payload).unwrap();
        let sha = hex::encode(Sha256::digest(&bytes));
        let cmd = make_cmd("1.0", &sha);
        assert!(prro_sidecar::validate_envelope(&cmd).is_ok());
    }

    #[test]
    fn tampered_payload_sha256_rejected() {
        let cmd = make_cmd("1.0", "deadbeef");
        let err = prro_sidecar::validate_envelope(&cmd).unwrap_err();
        assert!(err.contains("payload_sha256"), "got: {err}");
    }
}

#[cfg(test)]
mod idempotency {
    use prro_sidecar::repo::{PendingInsertResult, Repo};

    fn make_repo() -> Repo { Repo::open(":memory:").expect("in-memory repo") }

    #[test]
    fn fresh_key_returns_inserted() {
        let repo = make_repo();
        let r = repo.insert_pending_request("key-1", "FN001").unwrap();
        assert!(matches!(r, PendingInsertResult::Inserted));
    }

    #[test]
    fn accepted_key_returns_cached_json() {
        let repo = make_repo();
        repo.insert_pending_request("key-1", "FN001").unwrap();
        repo.accept_request("key-1", r#"{"status":1}"#).unwrap();
        match repo.insert_pending_request("key-1", "FN001").unwrap() {
            PendingInsertResult::DuplicateAccepted(json) => {
                assert_eq!(json, r#"{"status":1}"#);
            }
            other => panic!("expected DuplicateAccepted, got {other:?}"),
        }
    }

    #[test]
    fn different_key_returns_inserted() {
        let repo = make_repo();
        repo.insert_pending_request("key-A", "FN001").unwrap();
        repo.accept_request("key-A", r#"{"status":1}"#).unwrap();
        let r = repo.insert_pending_request("key-B", "FN001").unwrap();
        assert!(matches!(r, PendingInsertResult::Inserted));
    }

    #[test]
    fn fiscal_send_inner_contains_idempotency_wiring() {
        let src = include_str!("../src/bin/prro_sidecar.rs");
        assert!(src.contains("insert_pending_request"), "must call insert_pending_request");
        assert!(src.contains("accept_request"), "must call accept_request");
    }
}

#[cfg(test)]
mod dps_classifier_wire {
    use prro_sidecar::grpc_client::{classify_dps_status, DpsErrorCategory};

    #[test]
    fn status_ok_returns_none() {
        assert_eq!(classify_dps_status(1), None);
    }
    #[test]
    fn status_minus3_is_transient() {
        assert_eq!(classify_dps_status(-3), Some(DpsErrorCategory::Transient));
    }
    #[test]
    fn status_minus1_is_permanent() {
        assert_eq!(classify_dps_status(-1), Some(DpsErrorCategory::Permanent));
    }
    #[test]
    fn status_zero_is_permanent() {
        assert_eq!(classify_dps_status(0), Some(DpsErrorCategory::Permanent));
    }
    #[test]
    fn status_minus12_is_transient() {
        assert_eq!(classify_dps_status(-12), Some(DpsErrorCategory::Transient));
    }
}
