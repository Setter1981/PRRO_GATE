//! T3 (RULING 3) — document-derived offline/shift time budgets + the ONE
//! injected control-flow clock.
//!
//! Three budgets, ALL derived from durable rows (never from in-memory
//! wall-clock state) so they survive restart/crash by construction:
//!
//!   1. **24h shift duration** — `now − SHIFT_OPEN.business_ts` of the FN's open
//!      shift (`fiscal_documents WHERE shift_id = ? AND doc_type = 'SHIFT_OPEN'`).
//!      `shifts.opened_at` is a dead column (never stamped by the live write
//!      path), so the SHIFT_OPEN doc's `business_ts` is the durable open anchor
//!      (NOT NULL for both online and offline opens).
//!   2. **36h continuous offline session** — `now − offline_sessions.opened_at`
//!      of the FN's current OPEN session.
//!   3. **168h cumulative offline / calendar month** — RECOMPUTED-ON-READ by
//!      summing each offline session's `[opened_at, closed_at)` overlap with the
//!      current calendar month (UTC), plus the still-open session's
//!      `[opened_at, now)`. No mutable counter, no schema column, no migration.
//!
//! RULING 3.1/3.6: document business timestamps are the durable INPUTS; the ONE
//! injected [`Clock`] provides `now`. RULING 3.6 clock discipline: a backwards
//! clock input MUST NOT produce a negative budget or fail-open — every elapsed
//! computation clamps at zero.
//!
//! This module is PURE (no DB, no IO): callers read the durable timestamps and
//! hand them in. The SQL readers + the `run_staged` enforcement gate + the
//! supervisor auto-Z ticker live in their respective hot-zone files and all read
//! the SAME `Clock` seam (RULING 3.4/3.6).

use chrono::{DateTime, Datelike, TimeZone, Utc};

// ─── budget thresholds (INV-08 / INV-09 / INV-10, LEGAL_INVARIANTS.md) ───

/// 24h continuous shift duration (INV — LEGAL_INVARIANTS.md "24h shift limit").
pub const SHIFT_MAX_SECONDS: i64 = 24 * 3600;
/// 36h continuous offline session (INV-09).
pub const OFFLINE_SESSION_MAX_SECONDS: i64 = 36 * 3600;
/// 168h cumulative offline per calendar month (INV-10).
pub const OFFLINE_MONTH_MAX_SECONDS: i64 = 168 * 3600;

// ─── the ONE injected control-flow clock (RULING 3.6 / W5-1) ───

/// The single injected control-flow clock. Prod reads system UTC; tests inject a
/// [`FixedClock`] so the three budgets + the auto-Z ticker + the cert-gate all
/// advance deterministically. RULING 3.6: "one injected control-flow clock
/// drives admission/accumulation/auto-Z/cert-gate".
///
/// Deliberately trivial (`now_utc`) — document business timestamps remain the
/// durable inputs; this only supplies "now".
pub trait Clock: Send + Sync {
    fn now_utc(&self) -> DateTime<Utc>;
}

/// Production clock — system UTC.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Test clock — a fixed, injectable instant. Available in all builds (not
/// test-gated) so integration tests + the fuzzer's future `Op::AdvanceClock`
/// (RAGE W5) read the SAME seam. Cheap + `Copy`.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    now: DateTime<Utc>,
}

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }
    /// Build from an RFC-3339 string (test ergonomics). Panics on a malformed
    /// literal — tests pass constants, so a bad literal is a test bug.
    pub fn from_rfc3339(s: &str) -> Self {
        Self::new(
            DateTime::parse_from_rfc3339(s)
                .unwrap_or_else(|e| panic!("FixedClock::from_rfc3339({s:?}): {e}"))
                .with_timezone(&Utc),
        )
    }
    pub fn advance(&self, seconds: i64) -> Self {
        Self::new(self.now + chrono::Duration::seconds(seconds))
    }
}

impl Clock for FixedClock {
    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }
}

// ─── pure elapsed / overlap computation (clamped; RULING 3.6) ───

/// Parse an RFC-3339 durable business/session timestamp into UTC. Returns `None`
/// on a malformed value — the caller treats an unparseable anchor as
/// "no budget can be computed" (tracking is best-effort; enforcement is
/// fail-open ONLY for a genuinely-absent anchor, never for a backwards clock).
pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Non-negative elapsed seconds from `start` to `now`, CLAMPED at zero.
///
/// RULING 3.6: a backwards clock (`now < start`) yields `0`, never a negative
/// budget and never fail-open (a `0` budget is under every limit → tracking is
/// benign, enforcement does not fire, the auto-Z boundary is not falsely
/// crossed). This is the load-bearing clamp for the backwards-clock pin.
pub fn elapsed_seconds_clamped(start: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - start).num_seconds().max(0)
}

/// Overlap (seconds) of the half-open interval `[start, end)` with the calendar
/// month (UTC) that contains `now`. Used to recompute the 168h monthly
/// accumulator from durable session rows without a stored counter.
///
/// - `end == None` means the session is still open → clamp the end to `now`.
/// - All arithmetic clamps at zero (a row entirely outside the month, or a
///   backwards clock, contributes 0).
pub fn month_overlap_seconds(
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> i64 {
    let (m_start, m_next) = calendar_month_bounds(now);
    let seg_start = start.max(m_start);
    let seg_end = end.unwrap_or(now).min(m_next);
    (seg_end - seg_start).num_seconds().max(0)
}

/// `[first-of-month 00:00:00 UTC, first-of-next-month 00:00:00 UTC)` for the
/// month containing `now`.
fn calendar_month_bounds(now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let (y, m) = (now.year(), now.month());
    let start = Utc
        .with_ymd_and_hms(y, m, 1, 0, 0, 0)
        .single()
        .expect("first-of-month is always valid");
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let next = Utc
        .with_ymd_and_hms(ny, nm, 1, 0, 0, 0)
        .single()
        .expect("first-of-next-month is always valid");
    (start, next)
}

/// A single durable offline-session interval feeding the monthly recompute.
/// `opened_at` / `closed_at` are the RFC-3339 strings from `offline_sessions`.
#[derive(Debug, Clone)]
pub struct SessionInterval {
    pub opened_at: String,
    /// `closed_at` (CLOSED) or `drained_at`/absent — `None` for a still-open or
    /// abnormally-exited (ABORTED, no closed_at) session, which the recompute
    /// clamps to `now`.
    pub closed_at: Option<String>,
}

/// Sum the calendar-month overlap of every session interval (RULING 3.1 monthly
/// accumulator, recomputed-on-read). Unparseable `opened_at` rows are skipped
/// (best-effort tracking). Result is CLAMPED ≥ 0 by construction.
pub fn month_offline_seconds(intervals: &[SessionInterval], now: DateTime<Utc>) -> i64 {
    intervals
        .iter()
        .filter_map(|iv| {
            let start = parse_ts(&iv.opened_at)?;
            let end = iv.closed_at.as_deref().and_then(parse_ts);
            Some(month_overlap_seconds(start, end, now))
        })
        .sum()
}

// ─── per-budget enforcement toggle set (RULING 3.3, config-driven) ───

/// Which budgets ENFORCE (refuse new fiscal ops over-budget). Tracking is
/// ALWAYS on (RULING 3.2) regardless of these flags; only refusal is toggled.
/// Defaults ON (RULING 3.3). Auto-Z is UNCONDITIONAL (RULING 3.4) and does NOT
/// consult these flags.
#[derive(Debug, Clone, Copy)]
pub struct EnforcementToggles {
    pub shift_24h: bool,
    pub session_36h: bool,
    pub month_168h: bool,
}

impl Default for EnforcementToggles {
    fn default() -> Self {
        Self {
            shift_24h: true,
            session_36h: true,
            month_168h: true,
        }
    }
}

/// Which budget (if any) is over-limit AND enforced — the admission decision for
/// a NEW ordinary fiscal op. `None` ⇒ admit. Close-path ops are filtered by the
/// caller BEFORE reaching here (the legal close path is never blocked —
/// RULING 3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetRefusal {
    Shift24h,
    Session36h,
    Month168h,
}

/// The over-budget admission verdict for an ordinary op, given the three
/// computed budgets + the enforcement toggles.
///
/// A budget of `None` (no anchor / not applicable — e.g. no open offline
/// session) never refuses. Shift is checked first (it is the widest wedge — a
/// 24h shift traps everything), then session, then month.
pub fn admission_refusal(
    shift_seconds: Option<i64>,
    session_seconds: Option<i64>,
    month_seconds: Option<i64>,
    toggles: EnforcementToggles,
) -> Option<BudgetRefusal> {
    if toggles.shift_24h && shift_seconds.is_some_and(|s| s >= SHIFT_MAX_SECONDS) {
        return Some(BudgetRefusal::Shift24h);
    }
    if toggles.session_36h && session_seconds.is_some_and(|s| s >= OFFLINE_SESSION_MAX_SECONDS) {
        return Some(BudgetRefusal::Session36h);
    }
    if toggles.month_168h && month_seconds.is_some_and(|s| s >= OFFLINE_MONTH_MAX_SECONDS) {
        return Some(BudgetRefusal::Month168h);
    }
    None
}

impl BudgetRefusal {
    /// Stable HTTP-facing refusal code (the `OfflineRefused` → 503 family in
    /// `write_path::inline_map::codes`).
    pub fn code(self) -> &'static str {
        match self {
            BudgetRefusal::Shift24h => "SHIFT_DURATION_LIMIT_EXCEEDED",
            BudgetRefusal::Session36h => "OFFLINE_SESSION_LIMIT_EXCEEDED",
            BudgetRefusal::Month168h => "OFFLINE_MONTH_LIMIT_EXCEEDED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    // ── RULING 3.6 clock discipline: backwards clock clamps, never negative,
    //    never fail-open (pin 6 unit half) ──
    #[test]
    fn backwards_clock_clamps_to_zero_not_negative() {
        let start = t("2026-07-10T12:00:00Z");
        let now = t("2026-07-10T11:00:00Z"); // 1h BEFORE start
        assert_eq!(
            elapsed_seconds_clamped(start, now),
            0,
            "a backwards clock yields 0 elapsed, never a negative budget"
        );
        // And a 0 budget is under every limit → no refusal (fail-CLOSED means
        // we do NOT admit an over-budget op; a 0 budget is legitimately under).
        assert_eq!(
            admission_refusal(Some(0), Some(0), Some(0), EnforcementToggles::default()),
            None,
            "a clamped-0 budget never refuses (and never fail-opens a real breach)"
        );
    }

    // ── month accumulator recompute + rollover (pin 3 unit half) ──
    #[test]
    fn month_overlap_only_counts_current_calendar_month() {
        // A session spanning the June→July boundary; recompute for a July `now`
        // must count ONLY the July portion (the month accumulator RESETS at the
        // calendar boundary — RULING 3.3 "month rollover resets 168").
        let opened = t("2026-06-30T22:00:00Z"); // 2h in June
        let closed = Some(t("2026-07-01T02:00:00Z")); // 2h in July
        let now_july = t("2026-07-10T00:00:00Z");
        assert_eq!(
            month_overlap_seconds(opened, closed, now_july),
            2 * 3600,
            "only the July slice counts toward July's 168h"
        );
        // The SAME (already-closed) session, recomputed for a June `now`, counts
        // only its June slice — [22:00, month_end 24:00) = 2h — proving the
        // accumulator is per-calendar-month (the July slice is excluded).  For a
        // CLOSED session the overlap is bounded by the month end, not by `now`
        // (the July recompute above independently excludes the June slice).
        let now_june = t("2026-06-30T23:00:00Z");
        assert_eq!(
            month_overlap_seconds(opened, closed, now_june),
            2 * 3600,
            "for a June now, only the June slice [22:00,24:00) counts (July excluded)"
        );
    }

    #[test]
    fn month_open_session_clamps_end_to_now() {
        let opened = t("2026-07-05T00:00:00Z");
        let now = t("2026-07-06T00:00:00Z"); // 24h later, session still OPEN
        assert_eq!(
            month_overlap_seconds(opened, None, now),
            24 * 3600,
            "an open session contributes [opened_at, now) to the month"
        );
    }

    #[test]
    fn month_recompute_sums_intervals_clamped() {
        let now = t("2026-07-20T00:00:00Z");
        let ivs = vec![
            SessionInterval {
                opened_at: "2026-07-01T00:00:00Z".into(),
                closed_at: Some("2026-07-01T10:00:00Z".into()), // 10h
            },
            SessionInterval {
                opened_at: "2026-06-15T00:00:00Z".into(),
                closed_at: Some("2026-06-15T05:00:00Z".into()), // 0h in July
            },
            SessionInterval {
                opened_at: "bad-timestamp".into(), // skipped
                closed_at: None,
            },
        ];
        assert_eq!(
            month_offline_seconds(&ivs, now),
            10 * 3600,
            "recompute sums only July overlap; June + unparseable contribute 0"
        );
    }

    // ── admission verdict: each budget refuses at its own boundary, and the
    //    toggle gates the refusal (pin-support) ──
    #[test]
    fn admission_refuses_shift_at_24h_and_toggle_off_admits() {
        let on = EnforcementToggles::default();
        assert_eq!(
            admission_refusal(Some(SHIFT_MAX_SECONDS), None, None, on),
            Some(BudgetRefusal::Shift24h),
            "shift ≥ 24h with enforcement ON refuses"
        );
        assert_eq!(
            admission_refusal(Some(SHIFT_MAX_SECONDS - 1), None, None, on),
            None,
            "just under 24h admits"
        );
        let shift_off = EnforcementToggles {
            shift_24h: false,
            ..on
        };
        assert_eq!(
            admission_refusal(Some(SHIFT_MAX_SECONDS + 3600), None, None, shift_off),
            None,
            "toggle OFF admits even over-budget (RULING 3.3)"
        );
    }

    #[test]
    fn admission_refuses_session_and_month_at_boundaries() {
        let on = EnforcementToggles::default();
        assert_eq!(
            admission_refusal(None, Some(OFFLINE_SESSION_MAX_SECONDS), None, on),
            Some(BudgetRefusal::Session36h)
        );
        assert_eq!(
            admission_refusal(None, None, Some(OFFLINE_MONTH_MAX_SECONDS), on),
            Some(BudgetRefusal::Month168h)
        );
        // None budgets (no anchor / not applicable) never refuse.
        assert_eq!(admission_refusal(None, None, None, on), None);
    }

    #[test]
    fn fixed_clock_advances_deterministically() {
        let c = FixedClock::from_rfc3339("2026-07-10T00:00:00Z");
        assert_eq!(c.now_utc(), t("2026-07-10T00:00:00Z"));
        assert_eq!(
            c.advance(SHIFT_MAX_SECONDS + 1).now_utc(),
            t("2026-07-11T00:00:01Z"),
            "advance moves the injected now by exactly N seconds"
        );
    }
}
