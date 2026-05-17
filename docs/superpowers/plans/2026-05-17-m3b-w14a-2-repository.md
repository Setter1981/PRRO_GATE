# M3b W14a-2 — Repository whitelist + force seams + scanner tests

**Date**: 2026-05-17
**Base**: `rust-gateway` at `1d68a6d` (post-PR #65 merge — W14a-1 schema landed)
**Branch**: `m3b/w14a-2-repository`
**Spec authority**: `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` §4.1 (14 edges) + §4.5 (force seams) + §5.6 (matrix) + §16.3 (recovery classes) + §16.8 (1-cashier-per-shift) + §16.9 (senior close seam)

W14a-2 = **second slice** of M3b W14a impl per operator-pinned split (2026-05-17):
- W14a-1 ✅ — migration 016 + ShiftState enum + minimal compile fix (PR #65 merged at `1d68a6d`).
- **W14a-2 = THIS PLAN** — repository whitelist + force seams + scanner tests + channel-aware guard semantics + sign-time cashier enforcement.
- W14a-3 (future) — boot/reconciliation bridge + W14a-to-W10b typed BootError + 5 new W11-Δ replay fixtures.

---

## 1. Scope (load-bearing items)

### 1.1 shifts.rs repository — 14 whitelist edges + transition_state

Per spec §4.1 the **whitelist of allowed (from, to) edges** with rationale (drift-guard: locked-edge count = 14):

| # | From | To | Trigger |
|---|---|---|---|
| 1 | Created | Opening | online SHIFT_OPEN ingress |
| 2 | Created | OpenedLocalPendingDrain | offline SHIFT_OPEN ingress (Pattern C) |
| 3 | Opening | Opened | online send → DPS Ack (any attempt) |
| 4 | Opening | RequiresManualReconciliation | hard reject / operator give-up |
| 5 | OpenedLocalPendingDrain | Opened | W9b drain SHIFT_OPEN ack + empty backlog |
| 6 | OpenedLocalPendingDrain | RequiresManualReconciliation | drain SHIFT_OPEN rejected |
| 7 | OpenedLocalPendingDrain | ClosingLocalPendingDrain | offline Z_REPORT while open-doc pending drain |
| 8 | Opened | Closing | online Z_REPORT / SHIFT_CLOSE ingress |
| 9 | Opened | ClosingLocalPendingDrain | offline Z_REPORT (Pattern C) |
| 10 | Closing | Closed | online send → DPS Ack |
| 11 | Closing | Opened | `Authorization::DocumentReject` only (per §6.2) |
| 12 | Closing | RequiresManualReconciliation | hard reject |
| 13 | ClosingLocalPendingDrain | Closed | drain reached final Ack on Z + all backlog |
| 14 | ClosingLocalPendingDrain | RequiresManualReconciliation | drain rejected any backlog doc |

**Method**: `pub async fn transition_state(tx: &mut WriteTxConn<'_>, shift_id: ShiftId, from: ShiftState, to: ShiftState) -> Result<TransitionOutcome, SqlxError>` — CAS-style via UPDATE WHERE state = ?, returns:
- `TransitionOutcome::Applied` if rowcount == 1
- `TransitionOutcome::Forbidden { from, to }` if (from, to) ∉ whitelist (returned without touching DB)
- `TransitionOutcome::CASMiss { observed }` if rowcount == 0 (current state differs from from)

**Acceptance criteria**:
- All 14 edges round-trip via fixture test.
- All non-whitelisted (from, to) pairs return `Forbidden` without DB write.
- CAS-miss path verified via concurrent UPDATE.
- `with_immediate` envelope discipline preserved.

### 1.2 Force seams — 2 distinct methods per spec §4.5

```rust
// rust/prro/src/db/repositories/shifts.rs
pub async fn force_to_error_with_audit(
    tx: &mut WriteTxConn<'_>,
    shift_id: ShiftId,
    evidence_json: &str,
) -> sqlx::Result<()>;

pub async fn force_to_manual_reconciliation_with_audit(
    tx: &mut WriteTxConn<'_>,
    shift_id: ShiftId,
    evidence_json: &str,
) -> sqlx::Result<()>;
```

**Source-state restriction** per spec §4.5 (Round 6 A-H1) — both seams MUST refuse from forbidden sources:

| Seam | Allowed sources | Forbidden sources |
|---|---|---|
| `force_to_error_with_audit` | Opening / OpenedLocalPendingDrain / Opened / Closing / ClosingLocalPendingDrain / RequiresManualReconciliation | Created / Closed / Error |
| `force_to_manual_reconciliation_with_audit` | Opening / OpenedLocalPendingDrain / Opened / Closing / ClosingLocalPendingDrain | Created / Closed / Error / RequiresManualReconciliation |

Typed refusal: `ForceSeamForbiddenSource { current_state, attempted_seam }`.

**Audit events**:
- `SHIFT_FORCE_TO_ERROR` Critical
- `SHIFT_FORCE_TO_MANUAL_RECONCILIATION` Critical
- `SHIFT_FORCE_SEAM_REFUSED` Warning (when invoked from forbidden source)

**evidence_json validation**: parse-validate as `serde_json::Value` per spec §4.5; required fields `{operator_id, reason_code, free_text, timestamp_utc}`.

### 1.3 Senior cashier close seam per spec §16.9

```rust
pub async fn senior_cashier_close_shift_with_audit(
    tx: &mut WriteTxConn<'_>,
    shift_id: ShiftId,
    senior_cashier_id: CashierId,
    z_report_doc_id: DocumentId,
    evidence_json: &str,
) -> sqlx::Result<()>;
```

NOT a force seam — legitimate alternative close path via 5-ПРРО senior cashier role privilege. Internally:
1. Verify shift.state is in {Opened, ClosingLocalPendingDrain} (the only states where senior close is meaningful per §5.6).
2. UPDATE shifts SET state = 'CLOSED', closed_by_cashier_id = ?, z_report_document_id = ? WHERE shift_id = ?.
3. Emit `SHIFT_CLOSED_BY_SENIOR_CASHIER` Info audit with full evidence_json.

NOT validated against transition_state whitelist (it's a distinct seam with its own contract).

### 1.4 1-cashier-per-shift sign-time enforcement per spec §16.8

Per-doc verification at sign-time (stage_send / W9b drain dispatcher): refuse if `signed_by_cashier_id != shift.opened_by_cashier_id` EXCEPT for SHIFT_CLOSE / Z_REPORT (which may use senior cashier per §16.9).

**New typed error**: `SignerCashierMismatch { shift_id, expected_cashier_id, attempted_signer_id, doc_type }`.

**Implementation location**: stage_send (online path) + future W9b drain (offline path — partial in W14a-2 if feasible, else flagged for W9b PR).

**Note**: this requires plumbing `signed_by_cashier_id` through SigningContext. Scope may demand a separate W14a-2b slice if too invasive — operator decision.

### 1.5 Channel-aware OpenedLocalPendingDrain ops per spec §3.3 + §5.6

Current W14a-1 defensive arm in stage_acquire refuses ALL fiscal ops on OpenedLocalPendingDrain. W14a-2 correct semantics:

| Doc type | OpenedLocalPendingDrain | ClosingLocalPendingDrain |
|---|---|---|
| Sell / Return / ServiceIn / ServiceOut / XReport | **offline channel: ✓** / online channel: ✗-`SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED` | ✗-`POST_LOCAL_CLOSE_SALE_REFUSED` (per PR #62 §W10) |
| ZReport | ⤳-W10 (offline → edge 7 if pool conditions) | ✗-`ShiftClosingInFlight` |
| ShiftOpen | ✗-`ShiftAlreadyOpen` | ✗-`ShiftClosingInFlight` |
| ShiftClose | ✗-`OfflineShiftCloseNotSupported` (per §5.7 L2) | ✗-`ShiftClosingInFlight` |

Replace W14a-1 defensive arm with proper channel-aware matching.

### 1.6 Scanner tests (drift guards)

- `tests/shifts_no_silent_error_paths.rs` — scanner enforces:
  - **Tier (a)**: `transition_state` MUST NOT have any code path reaching `Error`.
  - **Tier (b)**: `transition_state` MUST reach `RequiresManualReconciliation` ONLY through edges 4 / 6 / 12 / 14.
- `tests/shifts_force_seam_source_guard.rs` — 18 (9 states × 2 seams) call-site matrix; permitted sources succeed, forbidden return `ForceSeamForbiddenSource`.
- `tests/shift_state_whitelist_matrix.rs` — verifies 14 edges + forbidden pairs per §4.4.

---

## 2. Out of W14a-2 scope (defer to W14a-3 / later)

- boot_phase recovery branches for 3 new states (W14a-3 + W14a-to-W10b BootError bridge per spec §11).
- 5 new W11-Δ replay fixtures (W14a-3).
- W12 KVT2 confirmation extension for SHIFT_OPEN (separate W12 follow-up).
- W10 policy guard primitive (separate W10a per spec §12.1).
- Operator UI / dashboard layer.

---

## 3. Implementation slices (sub-PRs if too large)

W14a-2 may split into:
- **W14a-2a**: shifts.rs whitelist + transition_state + force seams + senior close seam + dedicated repository tests.
- **W14a-2b**: scanner tests + channel-aware stage_acquire rewrite + sign-time cashier enforcement.

Operator decision at PR-open time.

---

## 4. Verification commands

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /mnt/d/PRRO_GATE-m3b-w14a-2/rust

# Targeted
cargo test -p prro --features test-support --test repo_shifts
cargo test -p prro --features test-support --test shifts_no_silent_error_paths
cargo test -p prro --features test-support --test shifts_force_seam_source_guard
cargo test -p prro --features test-support --test shift_state_whitelist_matrix

# Full
cargo test -p prro --features test-support
cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings
```

Expected: all green, no regression from W14a-1 baseline (573 passed at 1d68a6d).

---

## 5. PRRO invariants to preserve

- **I1** — no network/crypto in long SQLite tx → all transitions inside `with_immediate` envelope, no external calls.
- **I2** — one fiscal_number = one writer → W2 ReconcileGuard discipline + WriteTxConn-only access enforced.
- **I4** — idempotency → transitions CAS-protected (no double-apply).
- **I7** — canonical envelopes carry schema_version → unaffected (no payload changes).
- **I8** — recovery + state-machine correctness → 14-edge whitelist + scanner tests enforce; force seams audited; refused transitions surface as typed `Forbidden` not silent.
- **I9** — graceful shutdown → unaffected.
- **I10** — Checkbox-compat → unaffected.

---

## 6. Acceptance criteria (W14a-2 closes when)

1. shifts.rs has `transition_state` with 14-edge whitelist + 2 force seams + senior_cashier_close_shift_with_audit seam.
2. Scanner tests green: no-silent-error-paths + force-seam-source-guard + shift-state-whitelist-matrix.
3. stage_acquire channel-aware semantics correct per §5.6 matrix.
4. Sign-time cashier enforcement landed OR explicitly deferred to W14a-2b with operator agreement.
5. Full test suite green (573 + new tests, no regression).
6. PR review GO from operator.

---

## 7. Open questions — RESOLVED 2026-05-17

1. **Sign-time cashier enforcement** — **SPLIT to W14a-2b**. W14a-2a stays repository-core only; SigningContext plumbing kept off the whitelist-introduction risk surface.
2. **CashierId** — **minimal newtype** `pub struct CashierId(String)` for repository/seam APIs. No broad refactor; DB binds use `as_str()`. Implements `Display` + `FromStr` for ergonomic boundaries.
3. **Senior cashier validation** — **runtime existence check** in `cashier_certs` for the same fiscal_number. Role/privilege validation deferred to future role registry (UI responsibility for now).
4. **SHIFT_FORCE_SEAM_REFUSED audit shape** — **full evidence_json + metadata** (fiscal_number, shift_id, current_state, attempted_seam) per spec §8 forensic traceability. Size cap enforced; secrets-forbidden by convention (parse-validate as `serde_json::Value`).

## 8. Final W14a-2a scope (after operator triage)

W14a-2 split confirmed:
- **W14a-2a** (this PR): items §1.1, §1.2, §1.3, §1.6 from §1 above + `CashierId` newtype.
- **W14a-2b** (separate follow-up PR): items §1.4 (sign-time enforcement) + §1.5 (channel-aware stage_acquire).

W14a-2a in-scope items (final):
1. `CashierId` newtype foundation.
2. `transition_state` with 14-edge whitelist + `TransitionOutcome::{Applied, Forbidden, CASMiss}` enum.
3. `force_to_error_with_audit` + `force_to_manual_reconciliation_with_audit` with source-state restriction.
4. `ForceSeamForbiddenSource` typed error + `SHIFT_FORCE_SEAM_REFUSED` Warning audit with full evidence_json.
5. `senior_cashier_close_shift_with_audit` with runtime existence check against `cashier_certs(cashier_id, fiscal_number)`.
6. Scanner tests: `tests/shifts_no_silent_error_paths.rs` + `tests/shifts_force_seam_source_guard.rs` (18 cases = 9 states × 2 seams) + `tests/shift_state_whitelist_matrix.rs` (14 edges + 67 forbidden negatives = 81 total = 9×9).

Out of W14a-2a (deferred to W14a-2b):
- Channel-aware stage_acquire rewrite for OpenedLocalPendingDrain / ClosingLocalPendingDrain.
- Sign-time cashier enforcement (SigningContext plumbing).
