# W10 Final Audit Report

**Date:** 2026-05-11
**Reviewer:** senior Rust review (fresh session, full freeze + code + tests + migrations pass)
**Target:** PR #29 (merged `c31116c`), W10 = DPS error routing dispatch + MAC recovery
**Prompt:** `docs/superpowers/specs/W10_FINAL_REVIEW_PROMPT.md`
**Verdict:** ⚠️ Hardening recommended — accept for production pilot, one mandatory pre-W11 commitment.

---

## Findings summary

| Severity | Count | Items |
|---|---|---|
| HIGH | 0 | — |
| MED | 4 | MED-1 (lease drift), MED-2 (RetryClass enum coverage), MED-3 (mac_recovery_claim_counter_tx idempotency), MED-4 (W3 scanner pin) |
| LOW | 9 | LOW-1..LOW-9 (forensic + ergonomic polish) |
| NIT | 3 | NIT-1..NIT-3 (cosmetic) |

---

## Critical (mandatory pre-W11)

### MED-1 — "Single-writer-per-FN" docstring drift

**Evidence:** `mac_recovery.rs:286` + 4 other call sites claim "caller holds single-writer-per-FN lease". Actual lock is `ingress_inbox.rs:189` keyed on `request_id`, NOT `fiscal_number`. No FN-keyed Mutex/HashMap-of-locks in runtime.

**Why safe today:** single-worker dispatcher + `with_immediate` BEGIN IMMEDIATE serialisation globally.

**Why mandatory pre-W11:** multi-worker dispatcher (W11+) breaks the assumption. Two-tx MAC recovery split (MR-CLAIM → re-sign → MR-PERSIST) becomes race-vulnerable.

**Resolution options:**
- **(a)** Add real per-FN serialisation: in-memory `Arc<Mutex<HashMap<String, ()>>>` map OR DB-level `fn_writer_lock` table з `ON CONFLICT FAIL`.
- **(b)** Honestly rename invariant to "single-writer global via BEGIN IMMEDIATE + single inbox dispatcher" + re-audit MAC recovery / finalize / shift guards under weaker model.

**Action timing:** before W11 multi-worker dispatcher slice ships.

---

## High-value batchable (recommended fix-up commit)

| ID | Item | Effort |
|---|---|---|
| MED-2 | Add typed `RetryClass::Variant` assertions for `{WrapperBug, OperatorEscalation, MacRecovery}` (only string-asserted today) | ~30 LoC test |
| MED-3 | Direct repository test для `mac_recovery_claim_counter_tx` repeat-invocation idempotency (attempts=1 → second call rows_affected=0) | ~40 LoC test |
| MED-4 | Pin `mac_recovery.rs` in W3 static scanner module list | ~5 LoC |
| LOW-2 | Emit `MAC_RECOVERY_CLAIM_BURNED` audit inside MR-CLAIM envelope (positive trace for crypto-failure window) | ~15 LoC |
| LOW-6 | Add explicit `BEGIN; ... COMMIT;` envelope to `migrations/013_mac_recovery.sql` | 2 lines |
| LOW-7 | Truncate `error_message` at `stage_send::build_attempt_completion` before INSERT (avoid CHECK rejection silently losing trace) | ~10 LoC |

**Total estimated effort:** ~100 LoC. Suggests one bundled `fix(prro/W10): batch close 6 audit findings` commit.

---

## Deferred / spec cross-check

| ID | Item | Action |
|---|---|---|
| LOW-1 | Server -11 audit omits previous mode | Cross-check vs freeze §4.3 LOW 1 close — may be intentional contract |
| LOW-3..9 | Polish / forensic / test items | Defer to dedicated cleanup PR or fold into W11 prep |
| NIT-1..3 | Cosmetic | Defer indefinitely |

---

## Re-verified clean (audit trail)

Per the review's "Re-verified clean" section + "Contract compliance check":

- ✅ §2 main routing table (8/8 DpsError variants compile-time exhaustive).
- ✅ §2.1 Server sub-table (12 codes + unknown-code WrapperBug fallback + CRITICAL severity).
- ✅ Pattern B retry path (4-pre CAS + fx21 spy commit-before-send_chk).
- ✅ MAC orchestrator 4-step ordering (regex → MR-NO-TX → MR-CLAIM+re-sign → MR-PERSIST).
- ✅ Single-bit budget (DDL CHECK + helper CAS + dispatch flag = 3 layers).
- ✅ Server -11 structural ingress block via `stage_acquire.rs:92 RejectionReason::NodeOffline`.
- ✅ I1/I4/I8/I9 preserved (W3 scanner + monotonic retry_class + whitelist regression guard + CounterExhausted crash window).

---

## Verdict

> ⚠️ **Hardening recommended** — accept for production pilot on the current single-worker dispatcher, with one mandatory pre-W11 commitment (MED-1 lease scope resolution).

**Risk profile:**
1. **MED-1** is the highest forward-looking risk — today safe by combination of single-worker + global BEGIN IMMEDIATE; breaks if/when multi-worker dispatcher lands.
2. **LOW-2** is the most likely operator-confusion vector — silent crypto-failure crash window leaves no positive audit trace.
3. **LOW-7** loud CHECK rejection on long DPS error messages — silent trace coverage drop.
4. **MED-2..MED-4** test discipline gaps — reduce confidence that future refactors won't silently regress.

---

## Suggested next steps (audit-trail-anchored)

1. **MED-1 resolution slice** — separate task before W11 multi-worker dispatcher. Either real per-FN lease OR honest invariant rename.
2. **Bundled fix-up commit** (optional, can be batched into W11 prep) — close MED-2, MED-3, MED-4, LOW-2, LOW-6, LOW-7.
3. **Spec cross-check on LOW-1** against freeze §4.3 LOW 1 close — confirm "target-only audit" was intentional vs accidental.

---

## Audit metadata

- Review pass duration: ~15-20 min (one-shot fresh-session full read).
- Findings written with `file:line` citations (operator can `git log -L /pattern/:file` to trace each).
- Cross-referenced freeze §§2, 2.1, 3.5, 4.3, 9.2, 10.1.
- No code changes during audit (read-only).

This document IS the W10 audit-trail-anchored artefact. Use as input for:
- pilot deployment review board sign-off.
- W11 architectural pre-mortem (MED-1 commit precondition).
- post-incident forensic baseline.

---

## Resolutions

### MED-1 — RESOLVED 2026-05-12

**Resolution:** option (b-rename) per `docs/superpowers/specs/2026-05-11-med1-lease-scope-design.md`.

**Outcome:**
- New ADR-M3-A10 captures the M3a global-single-writer invariant, names today's enforcement mechanism (one tokio worker + `BEGIN IMMEDIATE` + per-row CAS + request-scoped inbox CAS), and enumerates the carry-forward obligations (FN-scope exclusion primitive, lock-leak recovery, contention metrics, docstring sweep, tests) that any future multi-worker slice MUST close in the same slice.
- 9 live Rust docstring sites updated to use "single-writer-per-FN **invariant**" instead of "single-writer-per-FN **lease**", each cross-referencing ADR-M3-A10. Sites: `mac_recovery.rs` (×2), `stage_send.rs` (×2), `stage_finalize.rs` (×2), `stage_acquire.rs` (clarifier on inbox-row lease vs FN-scope lock), `transport_trace.rs::complete_via_recovery_tx`, `boot_phase.rs::resume_sending_to_error_retryable`, `fiscal_documents.rs::fetch_finalize_inputs_tx`.
- Smoke test `rust/prro/tests/adr_m3_a10_exists.rs` pins ADR existence via `include_str!` and checks for required content markers (status header, carry-forward obligation names).
- **Audit refinement:** the original audit framed MED-1 as "pre-W11 mandatory" assuming W11 introduces multi-worker dispatch. Verification against `docs/superpowers/plans/2026-05-07-m3a-implementation.md` showed W11 is in fact deterministic-replay test fixtures (single-worker preserved). MED-1 is therefore documentation drift, not a correctness gap. Multi-worker dispatch arrives no earlier than M3b; ADR-M3-A10 §4 is the binding pre-condition for that slice.

**Verification:**
- `cargo fmt -p prro --check` — clean.
- `cargo clippy -p prro --all-targets --no-deps -D warnings` — clean.
- `cargo test -p prro` — 439 passed, 0 failed, 5 ignored (436 → 439; +3 ADR smoke tests).
- `grep -rn "single-writer-per-FN lease\|per-FN lease" rust/prro/src/` — zero hits.

**Frozen invariants check:**
- Invariants 1-10 in `CLAUDE.md` — unchanged. CLAUDE.md invariant 2 ("One `fiscal_number` = one logical single-writer write-path") describes the **logical** model; the global-single-writer mechanism satisfies it (strictly stronger than per-FN exclusion under one worker). No CLAUDE.md edit required.

### MED-2..MED-4, LOW-1..LOW-9, NIT-1..NIT-3 — OPEN

Not addressed in this slice. Carried forward per the original audit's "Suggested next steps":
1. Bundled fix-up commit (MED-2, MED-3, MED-4, LOW-2, LOW-6, LOW-7) — fold into W11 prep or open dedicated cleanup PR.
2. LOW-1 spec cross-check against W10 freeze §4.3 LOW 1 close.
