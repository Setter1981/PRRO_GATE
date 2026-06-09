//! CMS / cert time-parser hardening (sibling fixes from external-review round-5
//! of W4-Z3 — these parsers are NOT on the W4-Z3 signingTime path; they were
//! independently lenient).  Driven as INTEGRATION tests because prro_crypto's
//! in-crate `#[cfg(test)]` lib-tests are blocked by a separate pre-existing
//! `node_modules`-fixture `include_bytes!` (envelope.rs) — so the now-`pub`
//! low-level parsers are exercised directly here.

use prro_crypto::cms::calendar::{days_in_month, is_leap_year};
use prro_crypto::cms::envelope::parse_asn1_time;
use prro_crypto::cms::revocation::ymd_hms_to_unix;

#[test]
fn calendar_leap_and_days_in_month() {
    assert!(is_leap_year(2024) && is_leap_year(2000)); // /4, /400
    assert!(!is_leap_year(2023) && !is_leap_year(1900)); // not /4, /100-not-/400
    assert_eq!(days_in_month(2024, 2), 29); // leap February
    assert_eq!(days_in_month(2023, 2), 28); // non-leap February
    assert_eq!(days_in_month(2023, 4), 30);
    assert_eq!(days_in_month(2023, 1), 31);
    assert_eq!(days_in_month(2023, 13), 0); // out-of-range month → 0
    assert_eq!(days_in_month(2023, 0), 0);
}

#[test]
fn ymd_hms_rejects_impossible_day_of_month() {
    // Valid dates encode.
    assert!(ymd_hms_to_unix(2023, 1, 31, 0, 0, 0).is_some());
    assert!(ymd_hms_to_unix(2024, 2, 29, 0, 0, 0).is_some(), "leap Feb 29 valid");
    // Impossible day-of-month — previously Hinnant-normalised, now rejected.
    assert!(ymd_hms_to_unix(2023, 2, 31, 0, 0, 0).is_none(), "Feb 31");
    assert!(ymd_hms_to_unix(2023, 2, 29, 0, 0, 0).is_none(), "Feb 29 in non-leap year");
    assert!(ymd_hms_to_unix(2023, 4, 31, 0, 0, 0).is_none(), "Apr 31");
    assert!(ymd_hms_to_unix(2023, 1, 0, 0, 0, 0).is_none(), "day 0");
    assert!(ymd_hms_to_unix(2023, 13, 1, 0, 0, 0).is_none(), "month 13");
    // Leap second (60) allowed per X.680; 61 rejected.
    assert!(ymd_hms_to_unix(2016, 12, 31, 23, 59, 60).is_some(), "leap second 60");
    assert!(ymd_hms_to_unix(2016, 12, 31, 23, 59, 61).is_none(), "second 61");
}

#[test]
fn parse_asn1_time_rejects_trailing_junk_and_impossible_calendar() {
    // Valid UTCTime (0x17) / GeneralizedTime (0x18).
    assert_eq!(parse_asn1_time(b"491231235959Z", 0x17).unwrap(), "2049-12-31T23:59:59Z");
    assert_eq!(
        parse_asn1_time(b"20240229120000Z", 0x18).unwrap(),
        "2024-02-29T12:00:00Z",
        "leap-day GeneralizedTime"
    );
    // Trailing junk — was accepted (len-min check only), now rejected (exact len).
    assert!(parse_asn1_time(b"491231235959junkZ", 0x17).is_err(), "UTCTime trailing junk");
    assert!(parse_asn1_time(b"20491231235959junkZ", 0x18).is_err(), "GeneralizedTime trailing junk");
    // Fractional seconds (forbidden in RFC 5280 cert validity) — rejected.
    assert!(parse_asn1_time(b"20491231235959.5Z", 0x18).is_err(), "GeneralizedTime fractional");
    // Impossible calendar — was turned into a bogus RFC-3339 string, now rejected.
    assert!(parse_asn1_time(b"491331235959Z", 0x17).is_err(), "month 13");
    assert!(parse_asn1_time(b"490231235959Z", 0x17).is_err(), "Feb 31 (2049-02-31)");
    assert!(parse_asn1_time(b"491231255959Z", 0x17).is_err(), "hour 25");
    assert!(parse_asn1_time(b"491231236099Z", 0x17).is_err(), "minute 60");
    // Still rejects missing-Z / non-digit.
    assert!(parse_asn1_time(b"491231235959", 0x17).is_err(), "no trailing Z");
    assert!(parse_asn1_time(b"4912312359XXZ", 0x17).is_err(), "non-digit time field");
}
