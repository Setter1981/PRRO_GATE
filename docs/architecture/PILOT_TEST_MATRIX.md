# W4-Z4 Pilot Test Matrix — Pilot GATE

This document is a **pilot gate**, not a wish-list. Every row is either a binding
gate that must be satisfied before pilot authorization, or an explicitly marked
**gap-marker** for a control that does **not** exist in the Rust gateway today.

The single most important rule of this document:

> **WIRED vs UNWIRED honesty.** A control is **WIRED** only when a regression-pin
> test exists in the Rust gateway *today* and can pass against today's code. A
> control is **UNWIRED** when the production driver is absent — the test either
> does not exist, is `#[ignore]`'d / xfail, or would fail because there is no code
> path to exercise. **Do not assume Python-era parity.** The Python tree
> (`src/prro_gateway/`) is a dead reference; it does not count toward any gate.

Stack is **Rust-only**. There is no `pytest` / `ruff` gate. `--all-features` is
**not** used (it pulls `live-dps`, which must never run in CI).

Ground-truth source files are cited inline (`path:line`) so a reviewer can
re-confirm without trusting this prose.

---

## 0. Gate summary (go/no-go at a glance)

| Gate | Section | Type | Status |
|---|---|---|---|
| Static (fmt + clippy) | §3.1 | Static | Binding |
| Build / feature matrix + live-dps COMPILE-ONLY | §3.2 | Static | Binding |
| Concurrency & lease | §3.3 | WIRED | Binding |
| Concurrency-stress acceptance | §3.4 | WIRED / partial | Binding |
| Migration verification | §3.5 | WIRED | Binding |
| Offline & drain FSM | §3.6 | WIRED + UNWIRED | Mixed — **drain shift-safety UNWIRED in prod (DF-1, hard blocker)** |
| Rollback / crash injection | §3.7 | WIRED + UNWIRED | Mixed — see rows |
| Date / crypto / wire profile | §3.8 | WIRED + UNWIRED | Mixed — **HEAD signer detached / not live-accepted (DF-3, hard blocker)** |
| Shift lifecycle | §3.10 | UNWIRED (prod) | **Non-functional on HEAD (DF-1, hard blocker)** |
| Manual-recon | §3.11 | WIRED + UNWIRED | Mixed — **pager/snapshot UNWIRED (DF-4)** |
| Channel-lock / failover | §3.12 | UNWIRED | **Pilot risk — must accept or close** |
| Live-DPS smoke (manual) | §4 | Manual | Operator-gated |

> **PILOT VERDICT: NO-GO.** See the Hard-Blocker list in §5. The shift lifecycle
> is non-functional on HEAD and the offline Pattern C drain safety machinery is
> silently absent (DF-1); the live-DPS-accepted native ATTACHED signer is
> branch-only and HEAD's signer is detached + not live-accepted (DF-2/DF-3);
> `PRRO_FISCAL_MODE` is not harness-enforced (DF-5); INV-05/06 channel guards and
> INV-09/10 offline limits are UNWIRED.

Pilot is authorized **only** when every Binding gate passes and every UNWIRED
row that touches a frozen invariant (INV-05, INV-06, INV-09, INV-10) has an
explicit pilot-log risk-acceptance or a closing change. See §5.

---

## 1. The Static Gate (FACTS — Rust-only)

These are the **real** commands. The previous draft's `--all-features` and
`--all-targets` blanket commands were wrong: `--all-features` activates
`live-dps`, which must never execute in CI, and the `prro` clippy gate is scoped
to `test-support`, not all features.

The following must run with **ZERO** warnings or errors. If the static gate
fails, the codebase is rejected — no dynamic, operational, or live-DPS testing
may proceed.

```bash
# Format
cargo fmt --check

# Lint — prro crate is gated under test-support, NOT --all-features
cargo clippy -p prro --features test-support --tests -- -D warnings

# Lint — crypto crate
cargo clippy -p prro_crypto --all-targets -- -D warnings
```

Rationale for dropping `--all-features`: it would compile the `live-dps`
integration test and link the live-DPS path into the CI lint surface, which is
exactly the network-bearing code that must stay out of CI execution.

---

## 2. Build / Feature Matrix (FACTS) — including the live-dps COMPILE-ONLY gate

```bash
# Build tests under test-support
cargo build -p prro --tests --features test-support

# Run the prro test suite (test-support feature; NOT full workspace)
cargo test -p prro --features test-support

# live-DPS COMPILE-ONLY gate — MUST compile, MUST NOT execute in CI
# ⚠ PENDING MERGE — NOT runnable on rust-gateway HEAD (see note below)
cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run
```

**⚠ HEAD reality (CC2 — PENDING MERGE):** the third command above — and the
`live-dps` Cargo feature and `rust/prro/tests/live_dps_extended_smoke.rs` harness
it references — live **ONLY** on the unmerged branch
`feat/m4-w4-z3-dps-extended-smoke`. They are **NOT present on rust-gateway HEAD**.
So this binding static-gate command **cannot run on HEAD until that branch
merges**. The live harness that **exists on HEAD today** is
`live_smoke_w12_hardening.rs` (`--features test-support`, connect/probe-only,
dummy signing, **no CMS**).

**Critical contract for the third command (once merged):** the `--no-run` flag is
the gate. The `live-dps` integration test must **compile** in CI so that
wire-profile or DTO drift is caught at build time, but it must **never execute**
in CI — it makes real network calls to a DPS cabinet and would burn rate-limit
budget / risk unsafe host handling. Any CI configuration that drops `--no-run`
for this test is itself a pilot blocker.

The pilot scope is **RUST-ONLY**. The Python `src/prro_gateway` tree is a dead
reference contour. There is no `pytest`, `ruff`, `mypy`, or any Python gate in
this matrix.

---

## 3. Dynamic Test Gates

Legend for the **Type** column in every table below:

- **WIRED** — regression-pin test exists in the Rust tree today and can pass now.
- **UNWIRED** — gap-marker. No production driver / test absent or xfail; would
  fail today. The named test is the *target* to land, not a present asset.

The **Enforces (INV-NN)** column maps each row to the legal invariant from
`docs/LEGAL_INVARIANTS.md` (INV-01 .. INV-20) that it defends. `—` means the row
defends an engineering property rather than a numbered legal invariant.

### 3.1 / 3.2 Static & Build matrix

Covered above (§1, §2). These are static gates, not dynamic suites, but they are
listed first in the go/no-go summary because a red static gate halts everything.

### 3.3 Concurrency & Lease Invariants

One `fiscal_number` = one logical single-writer write-path (frozen invariant #2 /
INV-01). The lease model enforces this; these tests pin it.

| Test | Type | Enforces (INV-NN) | Notes |
|---|---|---|---|
| `test_db_lease_concurrent_exclusion` | WIRED | INV-01 | Two workers cannot hold the lease for one FN simultaneously. |
| `test_transaction_retry_backoff` | WIRED | INV-01, — | Raw `SQLITE_BUSY` becomes bounded typed retry, not undefined business behavior. |

### 3.4 Concurrency-Stress Acceptance Gate

Must exercise: same DB / same-FN contention / different-FN parallelism / same
request replay (idempotency) / reader-writer overlap / contention across
`stage_acquire` → `stage_sign` → `stage_send` → `stage_finalize` where
applicable.

**Acceptance criteria (all four must hold):**

| Acceptance criterion | Type | Enforces (INV-NN) |
|---|---|---|
| No state-machine corruption (no illegal `fiscal_documents.state` / `shifts.state` transition under contention) | WIRED | INV-01, INV-19 |
| No duplicate fiscal document (same FN + idempotency_key never yields two ledger rows) | WIRED | INV-07, INV-12 |
| No raw uncontrolled `SQLITE_BUSY` leaking as undefined outcome | WIRED | — |
| No stuck intermediate state (`SENDING` / `KVT1`) without a recovery owner | WIRED | INV-19 |

Idempotency reality (corrects the earlier "7 keys" invention): there is exactly
**one** idempotency surface — the composite `UNIQUE (fiscal_number,
idempotency_key)` index `ux_inbox_fn_idem`
(`rust/prro/migrations/002_fiscal_documents.sql:91`). The canonical content hash is
**not** an idempotency key (`runtime/ingress/dto.rs:304-317`). The separate
"DPS idempotency surface" is server-side `local_number` / `server_fiscal_no`
(`repositories/fiscal_documents.rs:74`), used by recovery to disambiguate
post-timeout — not a second local idempotency key.

### 3.5 Migration Verification

Migration runner uses checksum verification. The Rust pilot applies migrations via
`sqlx::migrate!("./migrations")` (`rust/prro/src/db/mod.rs:106`) — **not** the dead
Python `migrations/runner.py`. The gate is: schema N → current with no data damage.

| Test target | Type | Enforces (INV-NN) |
|---|---|---|
| Schema N → current applies cleanly, checksums verified | WIRED | — |
| `fiscal_documents` rows preserved across migration | WIRED | INV-02 |
| `shifts` rows preserved | WIRED | INV-03, INV-04 |
| `transport_trace_log` rows preserved | WIRED | INV-20 |
| Pending docs (`SENDING` / `ERROR_RETRYABLE`) remain recoverable post-migration | WIRED | INV-19 |
| Secure DB config (WAL, busy_timeout) preserved | WIRED | — |
| No historical timestamp damage (UTC stored values untouched) | WIRED | — |

**Schema-naming note (NO live drift in the pilot):** the live `offline_sessions`
column is `state`, with `CHECK (state IN ('OPENING','OPEN','DRAINING','CLOSED',
'ABORTED'))` — the CHECK source is `rust/prro/migrations/015_offline_normalize.sql:140`,
and the matching value set is the `OfflineSessionState` enum (`enums.rs:54`). (Repo
`offline_sessions.rs:225` is an UPDATE statement that *uses* the `state` column but
does **not** hold the CHECK — it is repo-uses-state evidence, not the CHECK source.)
The `status` column / `CLOSING` value are the **dead
pre-015 naming** (migration 004 / Python `sql/001_hot_store_init.sql`); migration `015` already
normalized `status`/`CLOSING` → `state`/`DRAINING`. So on the pilot path there is
**no `status`-vs-`state` drift and no stale `CLOSING`** — the enum in code
(`OfflineSessionState`: OPENING / OPEN / DRAINING / CLOSED / ABORTED) matches the
live `CHECK` exactly.

### 3.6 Offline & Drain State-Machine Gate

`OfflineSessionState`: `OPENING → OPEN → DRAINING → CLOSED / ABORTED`.
Pattern C durable offline doc state is `OFFLINE_LOCAL_ACK` (M3b naming).

| Test / case | Type | Enforces (INV-NN) | Ground truth |
|---|---|---|---|
| Pattern C local ACK lands `OFFLINE_LOCAL_ACK`, emits `OFFLINE_LOCAL_ACK_APPLIED` audit | WIRED | INV-13, INV-14 | transition `stage_offline_ack.rs:327`; audit `:350` (`:165` was the fn-entry line) |
| Code-pool exhaustion → typed `CodePoolExhausted` (error WIRED + tested) | WIRED | INV-11 | `offline_sessions.rs:408`; test `offline_session_code_pool.rs:201` |
| `CodePoolExhausted` → caller enters `STOP_MODE` (caller-routing) | **UNWIRED** | INV-11 | `stage_offline_ack.rs:315` propagates via `?` ("caller's responsibility"); **no production caller converts it to STOP_MODE**. Distinct from drain Tier-2 STOP_MODE (`trigger_tier_2_stop_mode`, `backlog_drain.rs:2074`, fires at `consecutive_holds >= 50` **AND** the `HeldAtSent` / `HeldAtKvt1` projection co-condition — `backlog_drain.rs:931-937`, so a bare counter ≥50 without that projection does NOT trip Tier-2; audit `OFFLINE_DRAIN_FN_STOP_MODE`). |
| Local ACK does **not** imply DPS acceptance (provisional until Ack) | WIRED | INV-13 | doc retained until drain |
| `OFFLINE_LOCAL_ACK` retained until DPS Ack (not finalized at local ACK) | WIRED | INV-14 | — |
| Drain preserves `lnd` ordering of backlog docs | WIRED | INV-02, INV-12 | `backlog_drain.rs` (`lnd` = persisted column name; `local_number` = wire-level name for the same value) |
| Drain-reject → `REQUIRES_MANUAL_RECONCILIATION` + Critical `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` escalation (INV-19) is **NON-FUNCTIONAL on HEAD** — the escalation is guarded by `shift_in_pending_drain` (`backlog_drain.rs:952`) which is **UNREACHABLE in production** because the pending-drain shift_state it keys on is never set (offline shift-creation edge 2 is UNWIRED; `stage_offline_ack` only READS `shift_state`; `node_state.current_shift_id` never set in prod). Code path IS present + test-pinned at `backlog_drain.rs:2191` (block `:2138-2191`), but no prod input reaches it. | **UNWIRED (prod)** | INV-19 | escalate guarded by `shift_in_pending_drain` `backlog_drain.rs:952`; emit `backlog_drain.rs:2191` |
| Drain doc-finalize of `OFFLINE_LOCAL_ACK` backlog → Ack (advances MAC chain). **CORRECTED (CF-R3): no backlog forms in prod, so this finalize arm is itself unreachable.** The ONLY production `upsert_initial` seeds `ShiftState::Closed` (`boot_phase.rs:1304`); orphan boot resolution only drives toward `CLOSED` (`boot_phase.rs:1491`); the sole `OPENED` write is a `#[cfg(test)]` fixture. Under `Closed`, `(Sell, Closed) → ShiftNotOpen` REFUSE (`stage_acquire.rs:897`) — SELL is **not admitted on either channel**. And offline is unreachable **end-to-end**: `node_state` has **no `Offline`/`GoingOffline` mode setter** (only `set_mode_blocked_tx` / `set_mode_stop_mode_tx`), `OfflineSessionService::open_session` has **zero production callers**, and `stage_offline_ack` requires `Opened` + an active offline session (`stage_offline_ack.rs:268-318`). So mode never flips `Offline`, no offline session opens, no `OFFLINE_LOCAL_ACK` doc forms, no backlog exists, and `drain()` early-returns "no active session" / "empty backlog" — the `Opened → None` finalize arm is itself **unreachable in prod**. This **strengthens the NO-GO**: the "drain finalizes backlog to Ack without escalation" scenario does not occur because no backlog forms. | **UNWIRED (prod — no backlog forms)** | INV-02, INV-12 | boot seeds CLOSED `boot_phase.rs:1304`; `(Sell, Closed)→ShiftNotOpen` `stage_acquire.rs:897`; offline guard `stage_offline_ack.rs:268-318`; `commit_finalize` match `backlog_drain.rs:2399-2418` |
| Edge-5/6/7/9/13/14 drain shift-TRANSITION + escalation (`OpenedLocalPendingDrain → Opened`, drain-reject → `RequiresManualReconciliation`) — the offline Pattern C shift SAFETY semantics (pending-drain online-ops lockout per §3.3, drain-reject escalation per INV-19) are **NON-FUNCTIONAL in production** because the pending-drain states they key on (`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`) are **never set** (edge-2 offline shift-creation UNWIRED; `current_shift_id` never set in prod). **CORRECTED (CF-R3): not even the doc-finalize runs in prod** — boot seeds `ShiftState::Closed` (`boot_phase.rs:1304`), so SELL is refused online (`(Sell, Closed) → ShiftNotOpen`, `stage_acquire.rs:897`) and offline is unreachable end-to-end (no `Offline`/`GoingOffline` mode setter, `open_session` has zero prod callers, `stage_offline_ack` requires `Opened` + active session `:268-318`). No offline session opens, no `OFFLINE_LOCAL_ACK` backlog forms, `drain()` early-returns — the whole drain (finalize + transition + escalation) is unreachable. **This is NOT a crash (fail-stop) — it is a silent ABSENCE of the safety machinery, which for a fiscal system is worse than a crash.** See §3.10 WL-1. **Hard pilot blocker.** | **UNWIRED (prod)** | INV-03, INV-19 | boot seeds CLOSED `boot_phase.rs:1304`; `(Sell, Closed)→ShiftNotOpen` `stage_acquire.rs:897`; offline guard `stage_offline_ack.rs:268-318`; escalate guard `backlog_drain.rs:952`; finalize match `backlog_drain.rs:2399-2418`; shifts.rs edges 5/6/7/9/13/14 |
| Edge-2 offline `SHIFT_OPEN` ingress creates pending-drain shift (`Created → OpenedLocalPendingDrain`, Pattern C) | **UNWIRED** | INV-03 | Shift CREATION has no production driver — `shifts::insert_created` (`shifts.rs:119`) has zero prod callers; see §3.10 WL-1 note. |
| **`test_offline_duration_36h_limit_freezes_ingress`** — continuous-offline ≥36h freezes ingress | **UNWIRED** | INV-09 | No `offline_session_started_at` reader; no `OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED`. |
| **`test_offline_monthly_168h_cap_blocks`** — monthly cumulative offline ≥168h blocks | **UNWIRED** | INV-10 | Column `current_month_offline_seconds` exists but **no enforcement reader**. |
| **`test_shift_duration_24h_limit_blocks`** — continuous-SHIFT-duration ≥24h blocks further ops (CF-R4) | **UNWIRED** | — (legal limit, `LEGAL_INVARIANTS.md §8` item 1 "active engineering risk") | **Distinct third limit** from INV-09 (36h continuous offline) and INV-10 (168h monthly) — this is the **24h continuous-shift wall**. **No enforcement in `src`** (no `24*3600` / `86400` / `MAX_SHIFT` / `shift_duration` reader). Same **risk-accept-or-enforce** framing as INV-09/INV-10: risk-acceptable only with explicit `bd` pilot sign-off; the only compliant exit is an **offline Z_REPORT local shift close (W10)** — itself UNWIRED. |
| Offline only entered on DPS-unreachable (auto-detect) | UNWIRED | INV-08 | Auto-offline classifier is stubbed today. |

### 3.7 Rollback / Crash-Injection Gate

Inject process kill at each write-path boundary and verify no orphan half-state
survives without a recovery owner, and that audit semantics (committed forensic
event vs rolled-back work) are correct.

| Injection point | Type | Enforces (INV-NN) |
|---|---|---|
| Fail during `stage_acquire` (lease/acquire tx) — no orphan lease, no orphan doc | WIRED | INV-01, INV-19 |
| Fail during signing-input pin — snapshot either fully pinned or absent (no drift) | WIRED | — |
| Fail after canonical XML build, before CMS persist — no half-signed doc | WIRED | INV-19 |
| Fail after CMS persist, before send — doc recoverable as `SIGNED`/`SENDING` | WIRED | INV-19 |
| Fail after `send_chk_v2` timeout, before KVT persist — recovery disambiguates via `server_fiscal_no`, no duplicate fiscalization | WIRED | INV-07, INV-19 |
| Verify rolled-back work emits no committed fiscal state; Critical forensic audits survive rollback | WIRED | INV-19 |

Signing snapshot immutability after pin is the load-bearing property here: once
inputs are pinned the document's signed bytes must not drift (see §3.8).

### 3.8 Date / Crypto / Wire-Profile Gate

**Crypto direction (corrects the earlier "encrypt outbound with DSTU 4145"
mislabel):**

- **OUTBOUND to DPS = CMS SIGNED** over **CP1251** canonical XML
  (`sign_cms_detached`, `crypto/provider.rs:50`; request field
  `SignCmsRequest.canonical_xml`, `crypto/provider.rs:33`). Signature algorithm is
  **DSTU 4145-2002** (PB-257 curve); hash is **GOST 34.311 / DSTU 7564 (Kupyna)**.
  **DSTU 4145 is a SIGNATURE scheme, NOT encryption.**
- **INBOUND (KVT2) = `unwrap_envelope` DECRYPT** of DPS `EnvelopedData`
  (`crypto/provider.rs:79`). **Encryption is INBOUND only.**
- **⚠ DETACHED (HEAD) vs ATTACHED (branch) — these are DIFFERENT signers and only
  one is live-DPS-accepted (DF-3).** rust-gateway **HEAD**'s `InProcessProvider`
  signs **DETACHED CMS** (no `eContent`) and is **NOT live-DPS-accepted**
  (`rust/prro/src/crypto/in_process.rs`, detached). The **ATTACHED CMS + `signingTime`
  signer that DPS actually accepted** exists **ONLY on the unmerged branch
  `feat/m4-w4-z3-dps-extended-smoke`** (`in_process.rs`, attached). So: **HEAD =
  detached signer (not live-accepted); the pilot-accepted native ATTACHED signer is
  branch-only, pending merge + external review.** This is a **hard pilot blocker**
  (see §5).
- The W4-Z3 live cycle was **signed-only** (**ATTACHED** CMS `SignedData`,
  `sendChkV2` accepted), proving the ФСКО path `PREPARED → SIGNED → SENT`. **This
  was the BRANCH ATTACHED signer, not HEAD's detached one.** The cycle is **PROVEN
  on branch `feat/m4-w4-z3-dps-extended-smoke` / PENDING MERGE to rust-gateway — NOT
  on HEAD** (the live harness + `live-dps` feature + the attached signer do not yet
  exist on rust-gateway HEAD).

| Test / case | Type | Enforces (INV-NN) | Notes |
|---|---|---|---|
| Kyiv DST EEST → EET fallback: repeated local hour must not collide on ordering/keys | WIRED | INV-02, — | internal chronology UTC-only; Kyiv is render-only |
| 2049 / 2050 UTCTime cliff (UTCTime → GeneralizedTime boundary) parsed correctly | WIRED | — | far-future `signingTime` fails fast |
| CMS `signingTime` Jan/Feb boundary correctness | WIRED | — | — |
| Cert validity UTCTime / GeneralizedTime parse | WIRED | — | crypto-correctness, not the passthrough/mock rule |
| DER `SET OF` lexicographic sorting in `signedAttrs` | WIRED | — | DPS rejects unsorted SET OF |
| Attached-CMS `eContent` = exact bytes hashed = exact bytes embedded | WIRED | — | `crypto/provider.rs:50` |
| **NO XML rebuild / reformat / reorder / re-encode after sign** — retry/resume use persisted signed bytes | WIRED | Crypto Immutable Rule (enabled-by INV-18) | byte-immutability post-pin |
| Invalid calendar dates (OCSP/CRL) fail loud, never silently normalize | WIRED | — | — |
| **`test_shift_open_refused_cert_expiry_under_36h`** — WebCheck 36h cert-expiry gate on SHIFT_OPEN | **UNWIRED** | INV-09 synergy (KeyRotationPending = INV-19 recovery class) | Spec §16.10; threshold = `NotAfter - now < 2160 min`. No Rust enforcement today. |

### 3.10 Shift Lifecycle Gate

`ShiftState` (9): `CREATED → OPENING → OPENED_LOCAL_PENDING_DRAIN → OPENED →
CLOSING_LOCAL_PENDING_DRAIN → CLOSING → CLOSED / REQUIRES_MANUAL_RECONCILIATION /
ERROR`.

The shift guard `check_shift_guard` (`stage_acquire.rs:845`) is a **162-cell**
matrix (9 DocType × 9 ShiftState × 2 channel), pinned by the oracle test
`check_shift_guard_matches_oracle_for_all_162_cells` (`stage_acquire.rs:1051`).
This guard is **WIRED and read-only**.

14 shift transition edges are declared in `shifts.rs:67`. The drain **transition**
edges **5, 6, 7, 9, 13, 14** are code-wired + tested (`backlog_drain.rs:2169`,
`:2498`; prod caller `app.rs:620`) — **but they are UNWIRED in production** (DF-1,
corrected re-tag). They only fire on a pending-drain `shift_state`
(`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`), which **production never
sets**: offline shift-creation edge 2 is UNWIRED, `stage_offline_ack` only READS
`shift_state`, and `node_state.current_shift_id` is never set in prod. Concretely:
the escalation `escalate_drain_to_manual` (the `current_shift_id` check, the
reviewer's claimed "crash" at `backlog_drain.rs:2155`) is reached **only**
`if shift_in_pending_drain` (`backlog_drain.rs:952`) → **UNREACHABLE in prod**.

**CORRECTED PREMISE (CF-R3) — the gateway cannot transact at all today, and no
backlog ever forms.** Earlier drafts said "`shift_state` statically seeded to
`Opened` so SELLs are admitted; `commit_finalize` matches `Opened → None`; the
drain finalizes the `OFFLINE_LOCAL_ACK` backlog docs to Ack." **That premise is
wrong.** The ONLY production `upsert_initial` seeds `ShiftState::Closed`
(`boot_phase.rs:1304`: `upsert_initial(pool, fn, NodeMode::Online,
ShiftState::Closed, 1)`); orphan boot resolution only drives toward `CLOSED`
(`boot_phase.rs:1491`); the only `OPENED` write in `src` is a `#[cfg(test)]`
fixture. Under `Closed`, `(Sell, Closed) → ShiftNotOpen` REFUSE
(`stage_acquire.rs:897`) — **SELL is not admitted on either channel**. Moreover
offline is unreachable **END-TO-END**: `node_state` has **no `Offline` /
`GoingOffline` mode setter** (only `set_mode_blocked_tx` / `set_mode_stop_mode_tx`),
`OfflineSessionService::open_session` has **zero production callers**, and
`stage_offline_ack` requires `Opened` + an **active offline session**
(`stage_offline_ack.rs:268-318`). So mode never flips `Offline`, no offline session
opens, no `OFFLINE_LOCAL_ACK` doc forms, no backlog exists, and `drain()`
early-returns "no active session" / "empty backlog" — the `Opened → None`
finalize arm (`commit_finalize`, `backlog_drain.rs:2399-2418`) is **itself
unreachable in prod**. So the offline Pattern C shift SAFETY semantics
(pending-drain online-ops lockout per §3.3, drain-reject → `RequiresManualReconciliation`
escalation per INV-19) are **silently ABSENT** — not a fail-stop crash, a silent
absence of the safety machinery, **which for a fiscal system is worse than a crash.**
This **strengthens the NO-GO**: the gateway **cannot transact at all on HEAD**
(online SELL refused on `Closed`; offline path unreachable end-to-end), and the
"drain finalizes backlog to Ack without escalation" scenario does not occur
because no backlog forms. This is a **hard pilot blocker** (see §5). Edges
**3 (Opening→Opened), 4, 8
(Opened→Closing), 10 (Closing→Closed), 11, 12** are whitelisted in the FSM but have
**no production caller** — they are UNWIRED.

**Edge 1 / edge 2 — shift CREATION is UNWIRED (WL-1).** Edge 2
(`Created → OpenedLocalPendingDrain`, offline `SHIFT_OPEN` ingress, Pattern C) and
edge 1 require a shift **row** to exist first, and **shift creation has no
production driver**: `shifts::insert_created` (`shifts.rs:119`) has **zero
production callers** (only `tests/repo_shifts.rs`); the sole `INSERT INTO shifts`
in `src` is under `#[cfg(test)]` (`backlog_drain.rs:2953`, `cfg(test)` opens at
`:2753`); `stage_offline_ack` only **reads** `ns.shift_state` to GUARD
(`stage_offline_ack.rs:268-289`), never creates or transitions a shift; and
`node_state.current_shift_id` is never set in production. So edge 2 (offline shift
CREATION) is **UNWIRED** — same class as online edges 3/8/10. This ties to the
**WL-1 shift-lifecycle gap**: the `shifts` table is not production-populated today,
so the WIRED drain transition edges above operate on **test-seeded shift rows
only**.

| Test / case | Type | Enforces (INV-NN) | Ground truth |
|---|---|---|---|
| 162-cell guard matches oracle for all cells | WIRED | INV-03, INV-15 | `stage_acquire.rs:1051` |
| `(ShiftOpen, Closed) → allow` | WIRED | INV-03 | guard matrix |
| `(ShiftOpen, *active*) → ShiftAlreadyOpen` | WIRED | INV-04 | guard matrix |
| `(Sell, Closed) → ShiftNotOpen` (fiscal op requires open shift) | WIRED | INV-03 | guard matrix |
| `(ZReport, OpenedLocalPendingDrain) → ZReportBlockedBacklogDrainPending` (audit `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED`) | WIRED | INV-15 | guard matrix; audit `stage_acquire.rs:782`, doc-comment `types.rs:213` |
| `(Sell, OpenedLocalPendingDrain, Online) → ShiftOpenPendingDrainOpRefused` | WIRED | INV-15 | guard matrix |
| NodeMode pre-guards: GoingOnline / Blocked / StopMode / CryptoDegraded refuse | WIRED | INV-19 | `stage_acquire.rs:293-362` (pre-guard match arms at `:293`/`:315`/`:331`/`:347`); the `check_shift_guard` call is the distinct downstream `:383` |
| Edge-5/6/7/9/13/14 drain shift-TRANSITION + escalation (`OpenedLocalPendingDrain → Opened`; drain-reject → escalation) — **NON-FUNCTIONAL in production** (DF-1): keyed on a pending-drain `shift_state` production never sets; escalation guarded by `shift_in_pending_drain` (`backlog_drain.rs:952`) is unreachable. **CORRECTED (CF-R3): not even the doc-finalize runs in prod** — boot seeds `ShiftState::Closed` (`boot_phase.rs:1304`) so SELL is refused online (`(Sell, Closed) → ShiftNotOpen`, `stage_acquire.rs:897`) and offline is unreachable end-to-end (no `Offline`/`GoingOffline` mode setter; `open_session` has zero prod callers; `stage_offline_ack` requires `Opened` + active session `:268-318`). No backlog forms → `drain()` early-returns → the whole `commit_finalize` (`:2399-2418`) `Opened → None` arm is unreachable. **No shift transition / escalation and does not crash.** Silent absence of the safety machinery — worse than a crash for a fiscal system. **Hard pilot blocker.** | **UNWIRED (prod)** | INV-03, INV-19 | boot seeds CLOSED `boot_phase.rs:1304`; `(Sell, Closed)→ShiftNotOpen` `stage_acquire.rs:897`; offline guard `stage_offline_ack.rs:268-318`; escalate guard `backlog_drain.rs:952`; finalize match `backlog_drain.rs:2399-2418`; shifts.rs edges 5/6/7/9/13/14 |
| Force / senior reconciliation seams (primitive WIRED + regression-pinned; **NO production driver / operator entry-point today — test-only**) | WIRED | INV-19 | `force_to_error_with_audit` `shifts.rs:444`; `force_to_manual_reconciliation_with_audit` `shifts.rs:575`; `senior_cashier_close_shift_with_audit` `shifts.rs:840`. No admin-CLI / runtime path invokes them; drain uses `shifts::transition_state` directly. |
| **`test_online_shift_open_creates_shift_and_advances_to_opened`** — online SHIFT_OPEN drives `Created/Opening → Opened` | **UNWIRED** | INV-03 | Edges 3/4/8/10/11/12 have no prod driver. W4-Z3 confirmed `node_state.shift_state` never opens online today. |
| **`test_active_shift_unique_index_present_in_rust_schema`** — partial-UNIQUE index forbids 2 active shifts/FN | **UNWIRED** | INV-04 | Only Python `sql/001_hot_store_init.sql:158` (dead Python contour, historical; old 3-state) had it — distinct from the Rust pilot migration `rust/prro/migrations/001_core_identities.sql`. Rust has only non-unique `ix_shifts_fn_state`. 9-state partial-unique index is aspirational. |
| **`test_shift_duration_24h_limit_blocks`** — continuous-SHIFT-duration ≥24h blocks further ops (CF-R4; also §3.6) | **UNWIRED** | — (legal limit, `LEGAL_INVARIANTS.md §8` item 1) | **Third distinct limit** beyond INV-09 (36h offline) / INV-10 (168h monthly): the **24h continuous-shift wall**. **No enforcement in `src`** (`24*3600` / `86400` / `MAX_SHIFT` / `shift_duration` grep empty). Only compliant exit is an **offline Z_REPORT local shift close (W10)** — itself UNWIRED. Same risk-accept-or-enforce framing as INV-09/INV-10. |

### 3.11 Manual-Reconciliation Gate

Manual recon is "ЧП из ЧП" — operator empirics over 4 years of UA PRRO production
show a near-zero observed rate, so the bias is strongly toward
HoldRetry/Rollback and the recovery taxonomy (AutoOfflineFallback /
TechSupportEscalation / KeyRotationPending / MacReseedRecovery /
TechSupportRepair) rather than `EscalateManual`. Per spec §16.7 / INV-19 there
are exactly three confirmed Manual-recon trigger families.

Recovery vocabulary ground truth: Rust `RetryClass` = { TerminalReject,
TransientRetry, FnConfigError, WrapperBug, ProbeRequired, MacRecovery,
OperatorEscalation }; `ErRedriveDecision` = { Redrive, BudgetExhausted,
EscalateManual, EscalateInconsistent, HoldProbeRequired, HoldIndeterminate }.
The 5-class operator taxonomy above is from spec §16.3 and is **not** a set of
Rust identifiers.

| Test / case | Type | Enforces (INV-NN) | Notes |
|---|---|---|---|
| Family (1): drain-reject of `OFFLINE_LOCAL_ACK` on pending-drain → `REQUIRES_MANUAL_RECONCILIATION` + Critical audit — code path present + test-pinned, but **NON-FUNCTIONAL in production (DF-1)**: the escalation is guarded by `shift_in_pending_drain` (`backlog_drain.rs:952`), and the pending-drain `shift_state` it keys on is **never set in prod** (edge-2 offline shift-creation UNWIRED; `current_shift_id` never set). So the primary Manual-recon surface is **silently absent** in prod, not reached. | **UNWIRED (prod)** | INV-19 | primary surface; escalate guard `backlog_drain.rs:952`; `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` emit `backlog_drain.rs:2191` (block `:2138-2191`) (also §3.6) |
| Family (3): operator force/senior seam declares shift unsalvageable (primitive WIRED + regression-pinned; **NO production driver — test-only; not operator-reachable on the pilot path today**) | WIRED | INV-19 | `force_to_manual_reconciliation_with_audit` `shifts.rs:575`; `senior_cashier_close_shift_with_audit` `shifts.rs:840` |
| Every Manual landing emits a **Critical audit** | WIRED | INV-19 | `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` `backlog_drain.rs:2191`; force seam `shifts.rs` |
| Forensic-snapshot capture + operator pager on Manual landing | **UNWIRED** | INV-19 | **NO code evidence** for a forensic-snapshot capture or operator pager — the Critical audit is the only wired effect (DF-4). Snapshot + pager are **operator-procedure only / deferred**. |
| **`test_ambiguous_online_shift_open_timeout_routes_manual`** — Family (2): ambiguous wire timeout on online SHIFT_OPEN (edge 4) or Z_REPORT (edge 12) → Manual | **UNWIRED** | INV-19 | Edges 4/12 are unreachable today; `shift_open_recovery.rs` is "proposed"/absent. |
| FN-deregistered-while-offline classifier (Case 10 subtype of family 1) | UNWIRED | INV-19 | Classifier not implemented; would route via family (1) once present. |

### 3.12 Channel-Lock / Failover Gate

This entire section is **UNWIRED** and is the sharpest pilot risk in the matrix.
Both rows touch frozen invariants and must be explicitly risk-accepted or closed
before pilot (see §5).

| Test / case | Type | Enforces (INV-NN) | Notes |
|---|---|---|---|
| **`test_channel_switch_refused_with_open_shift`** — channel switch forbidden with an open shift | **UNWIRED** | INV-05 (frozen invariant #3) | **NOT enforced in Rust.** This is a frozen invariant with no guard. |
| **`test_failover_allowed_only_outside_shift`** — transport failover permitted only outside an open shift | **UNWIRED** | INV-06 | Explicit GAP, tracked as `CHANNEL-FAILOVER-01`. |

---

## 4. Operations & Live-DPS Smoke (manual, operator-gated)

The live-DPS extended smoke is a **manual** acceptance against a DPS **test
cabinet** — it is not a CI gate and not mock validation. Full procedure
(pre-flight, environment contract, secret handling, host safety, execution
steps, PASS/FAIL classification, emergency off-switch) is in the runbook:

- **`/mnt/d/PRRO_GATE/docs/operations/LIVE_DPS_SMOKE_RUNBOOK.md`**

> Reference-path note: the W4-Z4 stabilization spec lists this artifact under
> `docs/ops/LIVE_DPS_SMOKE_RUNBOOK.md`. The canonical location for this gate is
> `docs/operations/LIVE_DPS_SMOKE_RUNBOOK.md` (alongside the existing
> `docs/operations/admin-runbook.md`). If the runbook is authored under
> `docs/ops/`, reconcile the path so this reference is not dangling — the old
> draft's bare `LIVE_DPS_SMOKE_RUNBOOK.md` (no directory) is the link that was
> previously broken.

**Branch-proven live cycle (W4-Z3, 2026-05-29) — PENDING MERGE, NOT on HEAD:**
native `prro_crypto` produced a full live WIRE cycle `SHIFT_OPEN → SELL →
Z_REPORT` accepted by the DPS test cabinet — `server_fiscal_no` values
`1g41M3jDt-Q` / `AOBSkplfIUU` / `L2AMnY2MkmA` via `sendChkV2` (ATTACHED CMS
SignedData). This was **PROVEN on branch `feat/m4-w4-z3-dps-extended-smoke` and is
PENDING MERGE to rust-gateway — it is NOT wired on rust-gateway HEAD** (the harness
`rust/prro/tests/live_dps_extended_smoke.rs` and the `live-dps` feature live only
on that branch). It is the reproducibility anchor the final pilot gate requires,
but the anchor **cannot be reproduced on HEAD until the branch merges**.

Once merged, the live test must **compile** in CI (`--no-run`, §2) and must
**never execute** in CI.

---

## 5. Pilot authorization gate (go/no-go contract)

> ## ⛔ PILOT VERDICT: **NO-GO** (current HEAD)
>
> The honest gate verdict on rust-gateway HEAD is **PILOT NO-GO**. The hard
> blockers below must each be closed (or have an explicit operator risk-accept
> where the row permits) before pilot authorization. This verdict supersedes any
> "accepted for pilot scope" language elsewhere in this matrix or in the
> RUNBOOK / PLAYBOOK.
>
> ### Hard blockers (each must be closed or explicitly risk-accepted)
>
> 1. **Shift lifecycle NON-FUNCTIONAL on HEAD (DF-1) — gateway cannot transact at
>    all (CF-R3).** Production bootstrap seeds `ShiftState::Closed`
>    (`boot_phase.rs:1304`; orphan boot only drives toward `CLOSED` `:1491`), so the
>    gateway **cannot transact at all today**: online SELL is refused on `Closed`
>    (`(Sell, Closed) → ShiftNotOpen`, `stage_acquire.rs:897`) and the **offline
>    path is unreachable end-to-end** — `node_state` has **no `Offline` /
>    `GoingOffline` mode setter** (only `set_mode_blocked_tx` / `set_mode_stop_mode_tx`),
>    `OfflineSessionService::open_session` has **zero production callers**, and
>    `stage_offline_ack` requires `Opened` + an active offline session
>    (`stage_offline_ack.rs:268-318`). Because mode never flips `Offline` and no
>    offline session ever opens, **no `OFFLINE_LOCAL_ACK` backlog forms**, so the
>    drain's whole machinery (doc-finalize + shift transition + `RequiresManualReconciliation`
>    escalation) is **unreachable** — `drain()` early-returns "no active session" /
>    "empty backlog", the escalation guard `shift_in_pending_drain`
>    (`backlog_drain.rs:952`) never fires, and the `commit_finalize` `Opened → None`
>    arm (`:2399-2418`) is never reached. There are **no online open/close drivers**
>    either. The offline Pattern C drain SAFETY machinery is therefore **silently
>    absent**. **Not a crash — a silent absence of safety, worse than a crash for a
>    fiscal system.** (§3.6 / §3.10)
> 2. **W4-Z3 native ATTACHED crypto unmerged + not externally reviewed (DF-2/DF-3).**
>    HEAD's `InProcessProvider` signs **DETACHED CMS** and is **NOT
>    live-DPS-accepted** (`rust/prro/src/crypto/in_process.rs`); the live-accepted
>    **ATTACHED** signer exists **only** on `feat/m4-w4-z3-dps-extended-smoke`,
>    pending merge + external review. (§3.8 / §4)
> 3. **`PRRO_FISCAL_MODE` not harness-enforced (DF-5).** The harness gates only on
>    `PRRO_LIVE_DPS=1` + host allowlist; a hard `PRRO_FISCAL_MODE=TEST` check is a
>    required pilot fix (deferred to the W4-Z3 branch). (§5 item 8)
> 4. **INV-05 / INV-06 channel guards UNWIRED (§3.12).** Channel-switch-with-open-shift
>    (INV-05, frozen invariant #3) and failover-outside-shift (INV-06) have no
>    guard. **Risk-accept only with an operations channel freeze.**
> 5. **INV-09 / INV-10 offline limits UNWIRED (§3.6, DF-6) — plus a third distinct
>    24h continuous-shift wall (CF-R4).** No production 36h-freeze (INV-09) or
>    168h-cap (INV-10) enforcement. **There is also no enforcement of the separate
>    24h continuous-SHIFT-duration limit** (`LEGAL_INVARIANTS.md §8` item 1, "active
>    engineering risk"; no `24*3600` / `86400` / `MAX_SHIFT` / `shift_duration`
>    reader in `src`) — its only compliant exit is an **offline Z_REPORT local shift
>    close (W10)**, itself UNWIRED. All three (24h / 36h / 168h) are **distinct**
>    limits. **Risk-accept only with offline descoped / operationally controlled and
>    explicit `bd` pilot sign-off.**
>
> ### Path to GO
>
> - **WL-1: full shift lifecycle** — including **offline `current_shift_id`** shift
>   creation/transition, **NOT online-only** — **OR** an **explicit offline descope**
>   that removes the Pattern C drain-safety dependency; **AND**
> - **WL-3: MAC internal-advance**; **AND**
> - **W4-Z3 merge + external review** of the native ATTACHED signer (closes
>   DF-2/DF-3 and unblocks the live-DPS `--no-run` and reproducibility sub-gates).

Pilot is authorized **only** when **all** of the following hold:

1. Static gate green (§1) — `fmt --check` + both scoped `clippy -D warnings`.
2. Build / feature matrix green (§2), including `live-dps … --no-run` compiling
   and being confirmed absent from CI execution. **⚠ CANNOT PASS ON HEAD TODAY:**
   the `live-dps … --no-run` sub-gate requires the harness + feature from branch
   `feat/m4-w4-z3-dps-extended-smoke`, which is **PENDING MERGE** to rust-gateway.
   This item is blocked until that branch merges.
3. All WIRED Binding gates green: §3.3, §3.4, §3.5, plus the WIRED rows of §3.6,
   §3.7, §3.8, §3.10, §3.11.
4. W4-Z3 live-DPS cycle reproducible (§4 anchor). **⚠ CANNOT PASS ON HEAD
   TODAY:** the anchor was proven on branch `feat/m4-w4-z3-dps-extended-smoke`
   (PENDING MERGE); the harness + `live-dps` feature are not on rust-gateway HEAD,
   so the cycle cannot be reproduced until that branch merges.
5. **Every UNWIRED row that touches a frozen invariant has an explicit
   pilot-log decision** — either a closing change or a written risk-acceptance.
   These are, by invariant:
   - **INV-05** channel-switch-with-open-shift guard (§3.12) — frozen invariant
     #3, currently unenforced.
   - **INV-06** failover-outside-shift (§3.12) — `CHANNEL-FAILOVER-01`.
   - **INV-09** 36h continuous-offline ingress freeze (§3.6) — **no production
     enforcement gate exists** (storage fields present, no reader).
   - **INV-10** 168h monthly offline cap (§3.6) — **no production enforcement gate
     exists** (`current_month_offline_seconds` column present, no enforcement
     reader).
   - **24h continuous-shift wall (CF-R4)** — a **third distinct limit** (not a
     numbered INV; `LEGAL_INVARIANTS.md §8` item 1, "active engineering risk").
     §3.6 / §3.10. **No production enforcement gate exists** (no `24*3600` /
     `86400` / `MAX_SHIFT` / `shift_duration` reader in `src`); its only compliant
     exit is an offline Z_REPORT local shift close (W10), itself UNWIRED.
   **INV-09 / INV-10 / 24h framing (DF-6 + CF-R4):** these are **risk-acceptable
   ONLY with an explicit `bd` pilot sign-off AND offline disabled / operationally
   controlled** — **no production 36h-freeze (INV-09), 168h-cap (INV-10), or
   24h-continuous-shift enforcement exists.** The 24h / 36h / 168h limits are
   **three distinct walls** that all carry an "active engineering risk" status in
   `docs/LEGAL_INVARIANTS.md §8`; this matrix binds them to named test targets, but
   the code-level reality is that all three are **UNWIRED**, so "accepted for pilot
   scope" is valid **only** under explicit sign-off with offline descoped or
   operationally controlled. Hard blocker if offline is in scope (see §5 list).
6. All other UNWIRED rows (online shift lifecycle drivers; active-shift
   partial-unique index; WebCheck 36h cert gate; ambiguous-timeout→Manual;
   FN-deregistered classifier) tracked in `bd` with `discovered-from:<W4-Z4
   epic-id>`. They are not individually pilot-blocking **provided** the WIRED
   offline/drain + Manual-recon surfaces below them hold, but they must be
   explicit, not silent.
7. 0 open Critical / High; every Medium fixed or accepted in `bd`; Low/Info
   tracked.
8. Live-DPS runbook complete with documented emergency stop path; secrets policy
   verified; test/production host separation verified.
   - **⚠ `PRRO_FISCAL_MODE` is NOT harness-enforced (DF-5).** The W4-Z3 harness
     does **not** define or check a `PRRO_FISCAL_MODE` env var — it gates only on
     `PRRO_LIVE_DPS=1` + the host allowlist (the local DB is seeded test-mode
     internally). Any RUNBOOK / PLAYBOOK text presenting `PRRO_FISCAL_MODE=TEST` as
     an enforced test/prod guard is **wrong**: it is a **manual operator preflight
     (NOT harness-enforced)**. A hard harness check for `PRRO_FISCAL_MODE=TEST` is a
     **required pilot fix (deferred to the `feat/m4-w4-z3-dps-extended-smoke`
     branch)**. Hard blocker (see §5 list).

This is the W4-Z4 contract: make the gateway **operationally pilot-ready**, not
merely implementation-complete. The matrix's job is to refuse false confidence —
every UNWIRED row above is a control that the Python era may have had and the
Rust gateway does **not** yet have.
