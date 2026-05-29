//! Gregorian calendar helpers for validating ASN.1 / fiscal date-time fields
//! (UTCTime / GeneralizedTime in OCSP/CRL validity, cert validity, signingTime)
//! without pulling in `chrono`.  Single-sourced here so `revocation::
//! ymd_hms_to_unix` and `envelope::parse_asn1_time` validate the day-of-month
//! identically — historically each parser checked only `day <= 31`, which let
//! impossible dates (Feb 31, Apr 31, Feb 29 in a non-leap year) through to a
//! Hinnant normalisation instead of failing loudly.

/// Proleptic-Gregorian leap-year test.
pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in the 1-based `month` of `year` (28/29/30/31).  Returns `0` for an
/// out-of-range month so a `day >= 1 && day <= days_in_month(y, m)` guard
/// rejects both a bad month and a bad day in one comparison.
pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}
