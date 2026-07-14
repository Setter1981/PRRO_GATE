//! T=112 ask-codes request builder.
//!
//! Builds the `<RQ V = '1'>…</RQ>` XML envelope that WebCheck
//! (`SendingOfflineChecksRobot.cs`) uses to request offline seed codes
//! from DPS.  The XML format is byte-identical to the live-accepted probe
//! captured on 2026-07-07 (sha256
//! `3df238207e472cf0443f3cadb802b4a20f418f249a7949a17950325bca383f1b`,
//! 206 bytes).
//!
//! ## Date-rule lockstep (-8 trap)
//!
//! DPS rejects with status -8 («XML дата не відповідає Check.date») when
//! the `<TS>` timestamp inside the XML differs from the gRPC envelope
//! `CheckEnvelope::date_time` that wraps the request.  [`T112Request`]
//! bundles both as a single type so callers physically cannot diverge
//! them.
//!
//! `comp_date` format is `yyyyMMddHHmmss` as a bare 14-digit `i64`
//! (e.g. `20260707224141`).  This is the Kyiv wall-clock, NOT a Unix
//! epoch.  [`kyiv_comp_date`] converts a `DateTime<Utc>` to this format
//! (`Europe/Kiev` via `chrono-tz`) — the SAME 14-digit encoding every
//! DPS document now uses for `<TS>` and `CheckEnvelope::date_time` (mirror
//! of `stage_send::kyiv_comp_date`).

use chrono::{DateTime, Datelike, Timelike, Utc};
use chrono_tz::Europe::Kiev;

// ── Errors ────────────────────────────────────────────────────────────────

/// Builder validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum T112Error {
    /// SIZE must be in 1..=2000.  0 is rejected (requesting zero codes is
    /// nonsensical); values above 2000 are silently clamped to 2000,
    /// mirroring WebCheck `SendingOfflineChecksRobot.cs:575`.
    InvalidSize { given: u32 },
}

impl std::fmt::Display for T112Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            T112Error::InvalidSize { given } => {
                write!(f, "T112 SIZE must be 1..=2000, got {given}")
            }
        }
    }
}

impl std::error::Error for T112Error {}

// ── Output type ───────────────────────────────────────────────────────────

/// Bundled output of [`build_t112_request`].
///
/// `xml` is the exact byte string to transmit as `CheckEnvelope::check_sign`
/// (the DPS gRPC field that carries the XML payload for T=112 requests).
/// `comp_date` is the same timestamp expressed as a `yyyyMMddHHmmss` `i64`
/// and must be placed in `CheckEnvelope::date_time` — both fields are
/// bundled here to prevent the DPS -8 date-mismatch rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct T112Request {
    /// Full XML string, UTF-8 encoded, ready to transmit.
    pub xml: String,
    /// `yyyyMMddHHmmss` Kyiv wall-clock digit-integer.  Matches the
    /// `<TS>` content embedded in `xml`.  Pass as `CheckEnvelope::date_time`.
    pub comp_date: i64,
}

// ── Builder ───────────────────────────────────────────────────────────────

/// Build a byte-exact T=112 ask-codes request.
///
/// # Arguments
/// * `fn_` — fiscal number string (`rro_fn`, e.g. `"4000162280"`)
/// * `tn` — tax number string (e.g. `"13667753"`)
/// * `di` — document index (1-based; usually 1 for the first batch)
/// * `size` — number of codes to request; 0 is rejected with
///   [`T112Error::InvalidSize`]; values above 2000 are silently clamped
///   to 2000 (WebCheck cap)
/// * `comp_date` — `yyyyMMddHHmmss` Kyiv-local digit-integer produced by
///   [`kyiv_comp_date`] or [`kyiv_comp_date_now`]; embedded verbatim in
///   `<TS>` and returned as `T112Request::comp_date` for the gRPC envelope
/// * `mac_hex` — lowercase hex MAC for `<MAC>…</MAC>`.  Pass `""` for
///   genesis (no prior session); `<MAC></MAC>` is always emitted (WebCheck
///   substitutes the placeholder `mmmaaaccc` unconditionally)
///
/// # Returns
/// [`T112Request`] on success, [`T112Error`] when `size == 0`.
pub fn build_t112_request(
    fn_: &str,
    tn: &str,
    di: u32,
    size: u32,
    comp_date: i64,
    mac_hex: &str,
) -> Result<T112Request, T112Error> {
    if size == 0 {
        return Err(T112Error::InvalidSize { given: 0 });
    }
    let clamped_size = size.min(2000);

    // Template mirrors WebCheck `SendingOfflineChecksRobot.cs:699`:
    //   string text2 = "<RQ V = '1'>";
    // Spaces around `=` in `<RQ V = '1'>` are verbatim.
    // All other attribute pairs use `name='value'` without spaces.
    // `<H SIZE='n'></H>` is an explicit open+close pair, NOT self-closing.
    let xml = format!(
        "<RQ V = '1'><DAT FN='{fn_}' TN='{tn}' ZN='' DI='{di}' V='1'>\
<C T='112'><H SIZE='{clamped_size}'></H></C>\
<TS>{comp_date}</TS></DAT><MAC>{mac_hex}</MAC></RQ>"
    );

    Ok(T112Request { xml, comp_date })
}

// ── Timestamp helpers ──────────────────────────────────────────────────────

/// Convert a `DateTime<Utc>` to the `yyyyMMddHHmmss` Kyiv-wall-clock
/// digit-integer that DPS expects in `<TS>` and `CheckEnvelope::date_time`.
///
/// This is the DPS `Check.date` encoding — the same 14-digit form used by
/// EVERY receipt / Z / service document (mirror of
/// `stage_send::kyiv_comp_date`). A prior fake-epoch encoding for ONLINE
/// receipts was WRONG (DPS `-8` "дата не відповідає Check.date") and was
/// removed 2026-07-12 (docs/DPS_MINUS8_DATE_AND_SHIFT_RECOVERY.md).
pub fn kyiv_comp_date(utc: DateTime<Utc>) -> i64 {
    let kyiv = utc.with_timezone(&Kiev);
    let y = kyiv.year() as i64;
    let mo = kyiv.month() as i64;
    let d = kyiv.day() as i64;
    let h = kyiv.hour() as i64;
    let mi = kyiv.minute() as i64;
    let s = kyiv.second() as i64;
    y * 10_000_000_000 + mo * 100_000_000 + d * 1_000_000 + h * 10_000 + mi * 100 + s
}

/// Production shorthand: `kyiv_comp_date(Utc::now())`.
pub fn kyiv_comp_date_now() -> i64 {
    kyiv_comp_date(Utc::now())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use sha2::{Digest, Sha256};

    // Live-accepted probe (2026-07-07) constants — hardcoded per spec.
    const LIVE_XML: &str = "<RQ V = '1'><DAT FN='4000162280' TN='13667753' ZN='' DI='1' V='1'><C T='112'><H SIZE='1'></H></C><TS>20260707224141</TS></DAT><MAC>e9e790ff8e0aa226ca4a85143aa0912fe499a0fa3dd468232419e3b77abab9c2</MAC></RQ>";
    const LIVE_SHA256: &str = "3df238207e472cf0443f3cadb802b4a20f418f249a7949a17950325bca383f1b";
    const LIVE_COMP_DATE: i64 = 20_260_707_224_141;
    const LIVE_MAC: &str = "e9e790ff8e0aa226ca4a85143aa0912fe499a0fa3dd468232419e3b77abab9c2";

    // ── (a) byte-pin against the live-accepted request ─────────────────

    #[test]
    fn byte_pin_full_equality() {
        let req = build_t112_request("4000162280", "13667753", 1, 1, LIVE_COMP_DATE, LIVE_MAC)
            .expect("valid inputs must succeed");
        assert_eq!(
            req.xml, LIVE_XML,
            "XML must be byte-identical to the live-accepted probe"
        );
    }

    #[test]
    fn byte_pin_length_206() {
        let req = build_t112_request("4000162280", "13667753", 1, 1, LIVE_COMP_DATE, LIVE_MAC)
            .expect("valid inputs must succeed");
        assert_eq!(req.xml.len(), 206, "live-accepted request is 206 bytes");
    }

    #[test]
    fn byte_pin_sha256() {
        let req = build_t112_request("4000162280", "13667753", 1, 1, LIVE_COMP_DATE, LIVE_MAC)
            .expect("valid inputs must succeed");
        let hash = Sha256::digest(req.xml.as_bytes());
        let hex = format!("{hash:x}");
        assert_eq!(
            hex, LIVE_SHA256,
            "sha256 must match the live-accepted probe digest"
        );
    }

    // ── (b) lockstep pin (-8 trap) ─────────────────────────────────────

    #[test]
    fn lockstep_comp_date_matches_ts_in_xml() {
        let req = build_t112_request("4000162280", "13667753", 1, 1, LIVE_COMP_DATE, LIVE_MAC)
            .expect("valid inputs must succeed");

        // Parse <TS>…</TS> from the XML to verify roundtrip.
        let ts_start = req.xml.find("<TS>").expect("<TS> must be present") + 4;
        let ts_end = req.xml.find("</TS>").expect("</TS> must be present");
        let embedded: i64 = req.xml[ts_start..ts_end]
            .parse()
            .expect("<TS> content must be a plain integer");

        assert_eq!(
            req.comp_date, embedded,
            "T112Request::comp_date must equal the <TS> integer (-8 trap)"
        );
    }

    // ── (c) comp_date format pin (DST mirrors stage_send.rs:2075/2092) ─

    #[test]
    fn kyiv_comp_date_summer_eest_plus3() {
        // 2026-07-15 10:00:00 UTC → EEST (+3) → 13:00:00 local
        let utc = Utc
            .with_ymd_and_hms(2026, 7, 15, 10, 0, 0)
            .single()
            .unwrap();
        assert_eq!(
            kyiv_comp_date(utc),
            20_260_715_130_000_i64,
            "summer EEST must be +3h"
        );
    }

    #[test]
    fn kyiv_comp_date_winter_eet_plus2() {
        // 2026-01-15 10:00:00 UTC → EET (+2) → 12:00:00 local
        let utc = Utc
            .with_ymd_and_hms(2026, 1, 15, 10, 0, 0)
            .single()
            .unwrap();
        assert_eq!(
            kyiv_comp_date(utc),
            20_260_115_120_000_i64,
            "winter EET must be +2h"
        );
    }

    #[test]
    fn kyiv_comp_date_is_not_epoch() {
        // Unix epoch for 2026-07-15 13:00:00 UTC ≈ 1_752_584_400.
        // The digit-integer 20_260_715_130_000 >> any Unix epoch in this era.
        let utc = Utc
            .with_ymd_and_hms(2026, 7, 15, 10, 0, 0)
            .single()
            .unwrap();
        assert!(
            kyiv_comp_date(utc) > 20_000_000_000_000_i64,
            "comp_date must be the 14-digit yyyyMMddHHmmss integer, not a Unix epoch"
        );
    }

    // ── FW-1 mutation teeth: nonzero-minute + live-clock ───────────────
    // `kyiv_comp_date` packs y/mo/d/h/mi/s into a 14-digit yyyyMMddHHmmss i64.
    // Survivor 1: the `+` before the `mi*100` term (`... + h*10_000 + mi*100 + s`)
    //   flipped to `-` SUBTRACTS the minutes contribution. Every pre-existing
    //   test uses a `:00:00` instant (mi=0 → mi*100=0 → `+0 ≡ -0`), so the flip
    //   is invisible. These pin non-zero minute/second instants.
    // Survivor 2: `kyiv_comp_date_now -> 0` stamps an epoch-zero Check.date; no
    //   non-live test invokes it. Pinned with a live-14-digit sanity bound.
    // This is the SAME civil-date family behind the live DPS `-8` stuck-shift
    // incident (docs/DPS_MINUS8_DATE_AND_SHIFT_RECOVERY.md).

    #[test]
    fn kyiv_comp_date_nonzero_minute_seconds_summer() {
        // 2026-07-15 10:37:29 UTC → EEST (+3) → 13:37:29 local.
        let utc = Utc.with_ymd_and_hms(2026, 7, 15, 10, 37, 29).single().unwrap();
        assert_eq!(
            kyiv_comp_date(utc),
            20_260_715_133_729_i64,
            "summer +3 non-zero minute/second: the mi*100 term must be ADDED, not subtracted"
        );
    }

    #[test]
    fn kyiv_comp_date_nonzero_minute_seconds_winter() {
        // 2026-01-15 10:45:11 UTC → EET (+2) → 12:45:11 local.
        let utc = Utc.with_ymd_and_hms(2026, 1, 15, 10, 45, 11).single().unwrap();
        assert_eq!(
            kyiv_comp_date(utc),
            20_260_115_124_511_i64,
            "winter +2 non-zero minute/second: the mi*100 term must be ADDED, not subtracted"
        );
    }

    #[test]
    fn kyiv_comp_date_day_boundary_offset_flips_calendar_day() {
        // 2026-07-15 21:30:00 UTC → EEST (+3) → 2026-07-16 00:30:00 local:
        // the +3 offset rolls the instant PAST Kyiv midnight into the next day.
        let utc = Utc.with_ymd_and_hms(2026, 7, 15, 21, 30, 0).single().unwrap();
        assert_eq!(
            kyiv_comp_date(utc),
            20_260_716_003_000_i64,
            "Kyiv +3 must cross into 2026-07-16 00:30:00 (minute term added)"
        );
    }

    #[test]
    fn kyiv_comp_date_now_is_live_14digit_not_zero() {
        // Kills `kyiv_comp_date_now -> 0`: a live yyyyMMddHHmmss is a 14-digit
        // int in this era; `0` (or any constant) fails the lower bound.
        let now = kyiv_comp_date_now();
        assert!(
            (20_000_000_000_000_i64..21_000_000_000_000_i64).contains(&now),
            "kyiv_comp_date_now must be a live 14-digit yyyyMMddHHmmss int, got {now}"
        );
    }

    // ── (d) genesis + SIZE bounds ──────────────────────────────────────

    #[test]
    fn genesis_empty_mac_emits_empty_mac_element() {
        let req = build_t112_request("4000162280", "13667753", 1, 1, LIVE_COMP_DATE, "")
            .expect("genesis (empty mac) must succeed");
        assert!(
            req.xml.contains("<MAC></MAC>"),
            "genesis MAC must emit <MAC></MAC>, got: {}",
            req.xml
        );
    }

    #[test]
    fn size_zero_returns_error() {
        let err = build_t112_request("4000162280", "13667753", 1, 0, LIVE_COMP_DATE, "")
            .expect_err("SIZE=0 must be rejected");
        assert_eq!(err, T112Error::InvalidSize { given: 0 });
    }

    #[test]
    fn size_2001_clamped_to_2000() {
        let req = build_t112_request("4000162280", "13667753", 1, 2001, LIVE_COMP_DATE, "")
            .expect("SIZE=2001 must succeed (clamped)");
        assert!(
            req.xml.contains("SIZE='2000'"),
            "SIZE=2001 must be clamped to 2000, got: {}",
            req.xml
        );
    }

    #[test]
    fn size_2000_accepted_at_cap() {
        let req = build_t112_request("4000162280", "13667753", 1, 2000, LIVE_COMP_DATE, "")
            .expect("SIZE=2000 must succeed");
        assert!(req.xml.contains("SIZE='2000'"));
    }

    #[test]
    fn size_1_accepted() {
        let req = build_t112_request("4000162280", "13667753", 1, 1, LIVE_COMP_DATE, "")
            .expect("SIZE=1 must succeed");
        assert!(req.xml.contains("SIZE='1'"));
    }
}
