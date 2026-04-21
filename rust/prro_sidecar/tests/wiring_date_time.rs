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
