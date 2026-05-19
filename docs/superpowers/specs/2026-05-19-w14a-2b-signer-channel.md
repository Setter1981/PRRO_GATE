# W14a-2b — Sign-time cashier enforcement + channel-aware stage_acquire

**Date:** 2026-05-19
**Status:** Implementation freeze — operator approval pending
**Predecessors:** W14a-1 (PR #65 merged `1d68a6d`), W14a-2a (PR #66 merged `67add6b`)
**Next downstream:** W9b (offline backlog drain — Pattern C orchestration)

---

## 1. Context + scope

PR #66 (W14a-2a) closed the shifts repository whitelist + force seams + senior cashier close.  Operator-chosen Path A (per `docs/superpowers/plans/2026-05-19-w14a2b-vs-w9b-ordering.md`) routes W14a-2b BEFORE W9b to close the W14a track on a clean shift-state surface.

**Base commit:** `origin/rust-gateway` `67add6b` (PR #66 merge).

**Scope is Rust-only.**  Editable:
- `rust/prro/src/...`
- `rust/prro/tests/...`
- `rust/prro/migrations/...`
- `docs/superpowers/...`

**Do NOT touch:**
- `sql/` (root) — historical Python contour migrations; not part of M3b Rust scope.
- `src/prro_gateway/...` (Python) — historical contour; out of scope.
- Python migrations — out of scope.

**In scope for W14a-2b (exactly 4 items):**

1. **§1.4 — Sign-time cashier enforcement** (closes W14a-2a §1.4 carry-forward; tracks spec §16.8).
2. **§1.5 — Channel-aware stage_acquire + mode guard rewrite** (replaces W14a-1's defensive arm at `stage_acquire.rs:401-412`).
3. **§1.5a — `stage_offline_ack` shift-state widening** (required corollary to §1.5: without it, offline ops in `OpenedLocalPendingDrain` are allowed by stage_acquire but refused at stage_offline_ack; spec §3.3 path is not actually enabled).
4. **§1.6 — TransitionOutcome::Conflict test polish** (closes W14a-2a R7 LOW carry-forward; ~10 LoC test).

**Out of scope (deferred):**
- W9b drain-time signer enforcement (offline path) — `signer_guard.rs` helper is structurally ready for W9b reuse; offline-replay invocation deferred to W9b PR.
- W10 — Z-report guard while offline backlog non-empty (coupled pool/backlog/edge 7 semantics).  W14a-2b explicitly blocks `(ZReport, OpenedLocalPendingDrain, Offline)` until W10 lands.
- W14a-3 role registry / SHIFT_CLOSE+Z_REPORT senior-cashier role policy (open question §16.8 in W14a-2a spec).
- Multi-cashier shift sharing / handoff semantics — future M3c.

---

## 2. Sign-time cashier enforcement design (§1.4)

### 2.1 Source-of-truth for `signed_by_cashier_id`

The ingress adapter MUST carry the signing cashier's id alongside the canonical fiscal command.  Two viable plumbing paths:

**Option A (recommended)** — extend `CanonicalFiscalCommand` (`rust/prro/src/services/write_path/types.rs:20`) with a new field:
```rust
pub struct CanonicalFiscalCommand {
    pub doc_type: DocType,
    pub business_ts: String,
    pub total_sum_kop: Option<i64>,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    /// W14a-2b §1.4 — operator/cashier id that will sign this document.
    /// Carries through stage 1 (PREPARED insert) → stage 3 (sign) → stage 4
    /// (send envelope).  None ONLY for system-context paths that cannot
    /// attribute (none currently; field is Option for forward compatibility).
    pub signed_by_cashier_id: Option<CashierId>,
}
```

**Option B** — add to `WorkerContext` at stage-1 close (extracted from a separate ingress-side struct).  Less invasive but requires synchronous availability at stage 1.

Decision: **Option A**, because (a) the field is conceptually part of the canonical command — auditors need it on PREPARED row; (b) carries through every stage uniformly without conditional re-resolution; (c) adapters already construct CanonicalFiscalCommand at ingress boundary.

`CashierId` is the newtype landed in W14a-2a in `rust/prro/src/db/models/ids.rs` (verified by grep — no separate `cashier_id.rs` file).  Implements `Display + FromStr + sqlx::Type` for ergonomic TEXT-column binding.

### 2.2 Persistence

Add column `signed_by_cashier_id TEXT` to `fiscal_documents` table via migration **`rust/prro/migrations/017_signed_by_cashier_id.sql`** (NOT root `sql/`):
```sql
ALTER TABLE fiscal_documents ADD COLUMN signed_by_cashier_id TEXT;
-- NULLABLE: pre-W14a-2b docs have no value; W9b drain may NULL it for
-- system-context replay paths (decision deferred to W9b).
```

No FK to `cashier_certs` (cross-FN binding is enforced at runtime via the same check as senior close in W14a-2a; ledger-level FK would block legitimate offline replay scenarios where the cashier cert was rotated post-issuance).

**Repository plumbing scope** (extended from initial draft to align with `stage_send`'s actual SendInputs-based path; operator corrections #2 + #4):

- `NewDocument` (stage 1 INSERT struct): add `signed_by_cashier_id: Option<CashierId>`.
- `DocumentRow` (read struct): add `signed_by_cashier_id: Option<CashierId>`; all SELECT statements carry the column.
- **`ShiftRow`** (operator correction #1 — was deferred in PR #66 L3, now in-scope for W14a-2b): expose `opened_by_cashier_id: CashierId` (currently `shifts.opened_by_cashier_id` is in DB schema since W14a-1 but the `ShiftRow` Rust struct + `shifts::get` / `shifts::get_tx` SELECTs do NOT carry it).  Without this, `signer_guard` cannot compare signer against opening cashier.  Add the field + widen both selectors.
- **`SendInputs`** (used by `stage_send` 4-pre — operator correction #2): add three fields **explicitly** (NOT "likely already present"):
  - `document_id: DocumentId` (sample helper at `stage_send.rs` already references `inputs.document_id` — verify and reconcile)
  - `shift_id: Option<ShiftId>` (NOT currently present in origin/rust-gateway SendInputs)
  - `signed_by_cashier_id: Option<CashierId>`
  - `doc_type` and `fiscal_number` already exist.
- `fiscal_documents::insert_prepared_tx` extended to bind `signed_by_cashier_id` from `NewDocument`.
- `fiscal_documents::fetch_send_inputs_tx` SELECT widened to include `signed_by_cashier_id`, `shift_id`, `document_id` (joining `shifts` table via `shifts.shift_id = fiscal_documents.shift_id` if needed; or reading `fiscal_documents.shift_id` directly — verify schema).
- Selectors: all `get_*` paths carry the new column into `DocumentRow`.

### 2.3 Refusal predicate (per spec §16.8)

`stage_send::run` takes only `doc_id` and loads `SendInputs` via `fetch_send_inputs_tx` (no `WorkerContext` available at this stage).  The signer guard must consume `SendInputs` + a loaded `ShiftRow` (or the helper loads the shift itself inside the same tx).

Helper signature in new `services/write_path/signer_guard.rs`:

```rust
pub fn enforce_signer_cashier_match(
    inputs: &SendInputs,
    shift: Option<&ShiftRow>,
) -> Result<(), SignerCashierMismatch> {
    // §16.9 bypass: SHIFT_CLOSE / Z_REPORT may be signed by senior cashier.
    // The senior_cashier_close_shift_with_audit seam (W14a-2a) has its own
    // runtime validation via cashier_certs; this helper trusts that layer.
    if matches!(inputs.doc_type, DocType::ShiftClose | DocType::ZReport) {
        return Ok(());
    }

    // Non-close fiscal ops require an active shift.  If shift_id is None
    // here, the doc was constructed outside a shift context — structural
    // refusal (caller bug: stage_acquire should have refused at guard time).
    let shift = shift.ok_or_else(|| SignerCashierMismatch::ShiftMissingForFiscalDoc {
        document_id: inputs.document_id,
        doc_type: inputs.doc_type,
    })?;

    let attempted = inputs.signed_by_cashier_id.as_ref().ok_or_else(||
        SignerCashierMismatch::SignerIdMissing { document_id: inputs.document_id }
    )?;
    let expected = &shift.opened_by_cashier_id;
    if attempted.as_str() != expected.as_str() {
        return Err(SignerCashierMismatch::Mismatch {
            shift_id: shift.shift_id,
            document_id: inputs.document_id,
            expected_cashier_id: expected.clone(),
            attempted_signer_id: attempted.clone(),
            doc_type: inputs.doc_type,
        });
    }
    Ok(())
}
```

**Position in stage_send pipeline:** inside the 4-pre `with_immediate` tx, BEFORE:
- envelope build
- CAS to `Sending`
- transport_trace insert
- `STAGE_SEND_INTENT_MARKED` audit
- wire send

Rationale: signer match is a precondition for fiscal_number burn / Pattern B `Sending` marker; refusal at this position prevents both wire I/O and state mutation.

**Audit + Ok-return contract** (operator correction #5):

The signer mismatch refusal MUST surface as an `Ok` variant (e.g. `PreOutcome::SignerRefused(_)`) on the stage_send pre-outcome enum, NOT `Err`.  Reason: `with_immediate` rolls back on `Err`; an `Err` return would discard the audit row.  Pattern mirrors W14a-2a `ForceSeamOutcome::ForbiddenSource` Ok-return.

Audit emit (`SIGNER_CASHIER_MISMATCH` Warning) happens INSIDE the same 4-pre tx, BEFORE returning `Ok(PreOutcome::SignerRefused(...))`.  This preserves the audit through commit.

### 2.4 Typed error

New enum in `services/write_path/signer_guard.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignerCashierMismatch {
    /// `SendInputs.signed_by_cashier_id` field is None.  Caller
    /// (ingress adapter) failed to attribute the signer.  Surfaces as
    /// `PreOutcome::SignerRefused` with `Ok` to preserve audit.
    #[error("stage send signer id missing for document {document_id:?}")]
    SignerIdMissing { document_id: DocumentId },

    /// Non-bypass fiscal doc has no resolvable shift binding —
    /// `shift` arg is `None` OR `inputs.shift_id` is `None`.
    /// Structural caller bug; stage_acquire should have refused at
    /// shift-guard time.  Surfaces as Ok refusal so audit row commits.
    #[error("stage send: non-bypass fiscal doc {document_id:?} of type {doc_type:?} has no resolvable shift_id")]
    ShiftMissingForFiscalDoc {
        document_id: DocumentId,
        doc_type: DocType,
    },

    /// MED-C3-1 — `inputs.shift_id` is `Some(X)` but the supplied
    /// `shift.shift_id` is `Y` with `X != Y`.  Caller loaded a sibling
    /// shift row (e.g. "any open shift on this FN") instead of the
    /// document's persisted shift binding.  Without this check,
    /// equality on cashier-id would pass tautologically if the FN has
    /// a single active cashier.
    #[error("stage send: shift_id mismatch for document {document_id:?} of type {doc_type:?} — expected {expected_shift_id:?}, supplied {supplied_shift_id:?}")]
    ShiftIdMismatch {
        document_id: DocumentId,
        doc_type: DocType,
        expected_shift_id: Option<ShiftId>,
        supplied_shift_id: ShiftId,
    },

    /// NIT-C3-2 — `inputs.fiscal_number != shift.fiscal_number`.
    /// Structural caller bug; defence-in-depth for W9b drain-time
    /// consumer that loads `SendInputs` + `ShiftRow` independently
    /// from persisted ledger rows.
    #[error("stage send: cross-FN binding for document {document_id:?} — inputs FN {inputs_fiscal_number}, shift FN {shift_fiscal_number}")]
    CrossFnMismatch {
        document_id: DocumentId,
        inputs_fiscal_number: String,
        shift_fiscal_number: String,
    },

    /// Signer cashier id ≠ shift's opening cashier id.  Operator/UI
    /// attribution error or signer pipeline misconfiguration.  Surfaces
    /// as Ok refusal + Warning audit.
    #[error("stage send signer {attempted_signer_id} does not match shift's opening cashier {expected_cashier_id} on shift {shift_id:?} doc_type {doc_type:?}")]
    Mismatch {
        shift_id: ShiftId,
        document_id: DocumentId,
        expected_cashier_id: CashierId,
        attempted_signer_id: CashierId,
        doc_type: DocType,
    },
}
```

**Bypass set (operator-resolved 2026-05-19, MED-C3-2):** the helper bypasses for `DocType::ShiftClose` / `DocType::ZReport` (per §16.9 senior-cashier seam owns validation) AND for `DocType::ShiftOpen` (at stage_send time the doc has `shift_id = NULL` because stage_acquire allows ShiftOpen only from `ShiftState::Closed` and resolves `active_shift = None`; the shift row is created during stage_finalize after DPS Ack, so the signer's `signed_by_cashier_id` BECOMES `shifts.opened_by_cashier_id` by construction).  Validating before creation is semantically empty.

**Routing:** all 5 variants surface as `Ok(PreOutcome::SignerRefused(SignerCashierMismatch::*))` from stage_send 4-pre tx.  Document state stays at `Signed` (Pattern B — no state mutation on refusal; matches W14a-2a force-seam Ok-return convention).  The refusal is observed by the worker loop; subsequent retry attempts that don't fix the signer id continue to fail (operator must reissue with correct cashier).

**Decision-order precedence:**

  1. `ShiftMissingForFiscalDoc` (shift arg = None OR inputs.shift_id = None)
  2. `ShiftIdMismatch` (inputs.shift_id != shift.shift_id)
  3. `CrossFnMismatch` (inputs.fiscal_number != shift.fiscal_number)
  4. `SignerIdMissing` (inputs.signed_by_cashier_id = None)
  5. `Mismatch` (signer != opening cashier)

Structural caller bugs (missing shift / wrong shift / cross-FN) surface BEFORE attribution bugs so the operator sees the root cause rather than a derived symptom.

### 2.5 Audit event

New audit event `SIGNER_CASHIER_MISMATCH`:

| Field | Value |
|---|---|
| entity_type | `fiscal_document` |
| entity_id | hex(document_id) |
| event_type | `SIGNER_CASHIER_MISMATCH` |
| severity | **Warning** |
| actor | variant-specific (see below) |
| payload | variant-specific (see below) |

**Variant-specific audit payloads (Commit 5 wiring):**

- `SignerIdMissing`: `{"fiscal_number": ..., "document_id": hex, "doc_type": ..., "variant": "SignerIdMissing", "refused_at_stage": "stage_send_pre"}` — no shift / signer fields (both unavailable).
- `ShiftMissingForFiscalDoc`: `{"fiscal_number": ..., "document_id": hex, "doc_type": ..., "variant": "ShiftMissingForFiscalDoc", "refused_at_stage": "stage_send_pre"}`.
- `ShiftIdMismatch`: `{"fiscal_number": ..., "document_id": hex, "doc_type": ..., "expected_shift_id": hex_or_null, "supplied_shift_id": hex, "variant": "ShiftIdMismatch", "refused_at_stage": "stage_send_pre"}`.
- `CrossFnMismatch`: `{"document_id": hex, "inputs_fiscal_number": ..., "shift_fiscal_number": ..., "variant": "CrossFnMismatch", "refused_at_stage": "stage_send_pre"}` — both FN values surfaced so forensic queries see the cross-FN binding explicitly.
- `Mismatch`: `{"fiscal_number": ..., "document_id": hex, "doc_type": ..., "shift_id": hex, "expected_cashier_id": ..., "attempted_signer_id": ..., "variant": "Mismatch", "refused_at_stage": "stage_send_pre"}`.

**Actor attribution per variant:**

- `Mismatch`: `actor = attempted_signer_id` (forensic queries surface "who tried").
- `SignerIdMissing` / `ShiftMissingForFiscalDoc` / `ShiftIdMismatch` / `CrossFnMismatch`: `actor = None` (no operator identity available OR root cause is a dispatcher / caller bug, not an operator action).

**Severity rationale:** Warning (not Critical) because (a) state never mutated — no fiscal commitment leaked; (b) refusal happens BEFORE wire send — no DPS interaction; (c) most likely cause is benign (operator typed wrong id or shift handoff misconfiguration).  Critical would be appropriate only if a wire send had already happened.

**Audit emit location** (operator correction #5): NOT in `error_routing.rs` (that module routes post-wire DPS outcomes).  Instead:
- The helper builds the payload but does NOT emit (helper is sync, audit_log writes are async).
- `stage_send::run` (4-pre tx) calls the helper, observes refusal variant, builds payload, and invokes `audit_log::append_tx(...)` INSIDE the same `with_immediate` tx, BEFORE returning `Ok(PreOutcome::SignerRefused(_))`.

Single audit per refused attempt; subsequent retries that don't fix the signer re-emit the same audit (intentional — each retry is independently auditable).

### 2.6 Offline path deferral

Per W14a-2 plan §1.4: "Implementation location: stage_send (online path) + future W9b drain (offline path — partial in W14a-2 if feasible, else flagged for W9b PR)."

W14a-2b **defers offline-path signer enforcement to W9b** because:
- The drain orchestrator (W9b) walks `OfflineLocalAck` docs that were persisted with `signed_by_cashier_id` (column carries through W7a `stage_offline_ack`).
- W9b's per-doc loop is the natural attachment point for the same enforcement helper (`enforce_signer_cashier_match`).
- Doing it in W14a-2b would require touching W9a `stage_send` widened source-state arm (`{Signed, ErrorRetryable, OfflineLocalAck}`) without W9b's surrounding drain semantics — risk surface too high without the corresponding integration tests.

W14a-2b spec **commits W9b to invoke `enforce_signer_cashier_match` at drain time** — the helper is hoisted to `services/write_path/signer_guard.rs` (new file) so both stage_send (W14a-2b consumer) and backlog_drain (W9b consumer) call the same code.

### 2.7 W7a/W7b interaction

**Operator correction #5** — this section previously stated "W14a-2b does NOT mutate W7a" which contradicts §3.7 (W14a-2b DOES widen `stage_offline_ack` shift-state allowed set).  Corrected scope:

- **Signer enforcement** at offline-local-ack time is **deferred to W9b** (drain-time validation).  Docs land in `OFFLINE_LOCAL_ACK` with whatever `signed_by_cashier_id` the ingress adapter provided; mismatch surfaces only at drain time.
- **Shift-state widening** of `stage_offline_ack` (allowed set `Opened | OpenedLocalPendingDrain` for regular fiscal docs) **IS in W14a-2b scope** — see §3.7.

Rationale for deferring signer enforcement on offline path:
- Offline path is the resilience surface — refusing offline ops at ingress would defeat the purpose (operator can't reach a re-attribution path while offline).
- The persisted `signed_by_cashier_id` carries the operator's intent forward; W9b drain validates against the shift's `opened_by_cashier_id` at sync time.
- This means an offline op with a wrong cashier id will appear as `OFFLINE_LOCAL_ACK` locally but FAIL drain — the operator gets feedback only at return-online.

Open question (deferred): should W7a emit an `OFFLINE_SIGNER_PROVISIONALLY_ACCEPTED` audit event when `signed_by_cashier_id != shift.opened_by_cashier_id` to flag the future drain refusal at offline time?  **Default: no** (avoid audit noise; operator UX can read the mismatch from doc + shift row).

---

## 3. Channel-aware stage_acquire design (§1.5)

### 3.1 Current state (post-W14a-1)

`stage_acquire.rs:401-412` (W14a-1 defensive arm) refuses ALL fiscal ops (Sell/Return/ServiceIn/ServiceOut/CashWithdrawal/XReport) against both `OpenedLocalPendingDrain` AND `ClosingLocalPendingDrain`:

```rust
(
    DocType::Sell | DocType::Return | DocType::ServiceIn | DocType::ServiceOut
        | DocType::CashWithdrawal | DocType::XReport,
    ShiftState::OpenedLocalPendingDrain | ShiftState::ClosingLocalPendingDrain,
) => Some(RejectionReason::ShiftNotOpen { current: shift_state }),
```

This is W14a-1's "minimal compile coverage" — the comment ALREADY anticipates W14a-2's channel-aware semantics.

### 3.2 Target semantics (per spec §3.3 + §5.6)

| Doc type | OpenedLocalPendingDrain | ClosingLocalPendingDrain |
|---|---|---|
| Sell / Return / ServiceIn / ServiceOut / XReport / CashWithdrawal | **offline channel ✓** / online ✗ `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED` | ✗ `POST_LOCAL_CLOSE_SALE_REFUSED` (PR #62 §W10) |
| ZReport | ⤳ W10 (offline → edge 7 if pool conditions) | ✗ `ShiftClosingInFlight` |
| ShiftOpen | ✗ `ShiftAlreadyOpen` | ✗ `ShiftClosingInFlight` |
| ShiftClose | ✗ `OfflineShiftCloseNotSupported` (§5.7 L2) | ✗ `ShiftClosingInFlight` |

### 3.3 Mode guard rewrite + channel derivation

**Operator correction #1**: `stage_acquire` currently rejects ALL non-`Online` modes BEFORE `check_shift_guard`:

```rust
// rust/prro/src/services/write_path/stage_acquire.rs (current)
if node_state.mode != NodeMode::Online {
    return reject(... RejectionReason::NodeOffline ...)
}
```

Without changing this guard, channel-aware `check_shift_guard(_, _, Channel::Offline)` is **unreachable** at runtime — the offline branch is dead code.

**W14a-2b mode guard rewrite** (replaces the binary `mode != Online` reject):

```rust
let channel = match node_state.mode {
    NodeMode::Online => Channel::Online,
    NodeMode::Offline | NodeMode::GoingOffline => Channel::Offline,
    NodeMode::GoingOnline => {
        // Drain in progress — no new fiscal ops (wait for return-online completion).
        return reject(... RejectionReason::NodeGoingOnlineDrainInFlight ...);
    }
    NodeMode::Blocked => return reject(... RejectionReason::NodeBlocked ...),
    NodeMode::StopMode => return reject(... RejectionReason::NodeStopMode ...),
    NodeMode::CryptoDegraded => return reject(... RejectionReason::NodeCryptoDegraded ...),
};
```

This produces an explicit `channel: Channel` value passed into `check_shift_guard(doc_type, shift_state, channel)`.

**New RejectionReason variants** (mode-side, not shift-side):
- `NodeGoingOnlineDrainInFlight` — drain is in progress, retry after `GoingOnline → Online`.
- `NodeBlocked` — node mode is `Blocked` (operator manual recovery).
- `NodeStopMode` — node mode is `StopMode` (legal hold).
- `NodeCryptoDegraded` — crypto subsystem degraded (key expiry imminent, sidecar down).

The existing `RejectionReason::NodeOffline` is **REPLACED** — Offline mode no longer refuses at this guard.  Migration path: remove the variant after grep confirms no callers; OR mark deprecated with `#[deprecated]` and keep for one cycle.  **Default: remove** (only stage_acquire emitted it; clean cut).

**Audit emit on mode refusals:** existing `STAGE_ACQUIRE_NODE_REFUSED` audit shape extended with `mode` field (was: implicit Offline only).

**Channel enum placement**: `services/write_path/types.rs` (per operator correction OQ3 resolved).  NOT a DB enum; never persisted.

### 3.4 New check_shift_guard signature

Extend the function to take `channel: Channel` as a third parameter:

```rust
fn check_shift_guard(
    doc_type: DocType,
    shift_state: ShiftState,
    channel: Channel,
) -> Option<RejectionReason> {
    use Channel::*;
    match (doc_type, shift_state, channel) {
        // Terminal / operator-action arms — channel-irrelevant.
        (_, ShiftState::Error, _) => Some(RejectionReason::ShiftInError),
        (_, ShiftState::RequiresManualReconciliation, _) => {
            Some(RejectionReason::ShiftRequiresOperatorAttention)
        }

        // Shift-management ops.
        (DocType::ShiftOpen, ShiftState::Closed, _) => None,
        (DocType::ShiftOpen, _, _) => Some(RejectionReason::ShiftAlreadyOpen),
        (DocType::ShiftClose, ShiftState::Opened, _) => None,
        (DocType::ZReport, ShiftState::Opened, _) => None,

        // Mid-transition blocks everything.
        (_, ShiftState::Created | ShiftState::Opening | ShiftState::Closing, _) => {
            Some(RejectionReason::ShiftNotOpen { current: shift_state })
        }

        // Regular fiscal ops in Opened (online + offline both work).
        (
            DocType::Sell | DocType::Return | DocType::ServiceIn | DocType::ServiceOut
                | DocType::CashWithdrawal | DocType::XReport,
            ShiftState::Opened,
            _,
        ) => None,

        (
            DocType::Sell | DocType::Return | DocType::ServiceIn | DocType::ServiceOut
                | DocType::CashWithdrawal | DocType::XReport,
            ShiftState::Closed,
            _,
        ) => Some(RejectionReason::ShiftNotOpen { current: shift_state }),

        // ── W14a-2b §1.5 — Channel-aware OpenedLocalPendingDrain ──
        // Offline channel: fiscal ops PROCEED (Pattern C resilience surface).
        (
            DocType::Sell | DocType::Return | DocType::ServiceIn | DocType::ServiceOut
                | DocType::CashWithdrawal | DocType::XReport,
            ShiftState::OpenedLocalPendingDrain,
            Offline,
        ) => None,

        // Online channel: refuse with channel-aware reason.
        (
            DocType::Sell | DocType::Return | DocType::ServiceIn | DocType::ServiceOut
                | DocType::CashWithdrawal | DocType::XReport,
            ShiftState::OpenedLocalPendingDrain,
            Online,
        ) => Some(RejectionReason::ShiftOpenPendingDrainOpRefused),

        // ZReport on OpenedLocalPendingDrain — operator correction #3:
        // BLOCK in W14a-2b for both channels until W10/W9b coupled
        // pool/backlog/edge-7 logic exists.  Allowing Offline ZReport
        // here would create a window where Z-report can be issued while
        // offline backlog non-empty (pre-W10).  W10 later replaces this
        // refusal with the proper coupled guard.
        (DocType::ZReport, ShiftState::OpenedLocalPendingDrain, Offline) => {
            Some(RejectionReason::ZReportBlockedBacklogDrainPending)
        }
        (DocType::ZReport, ShiftState::OpenedLocalPendingDrain, Online) => {
            Some(RejectionReason::ShiftOpenPendingDrainOpRefused)
        }

        // ── ClosingLocalPendingDrain — post-local-close lockout ──
        // ALL ops refused; channel-irrelevant (PR #62 §W10 contract).
        (
            DocType::Sell | DocType::Return | DocType::ServiceIn | DocType::ServiceOut
                | DocType::CashWithdrawal | DocType::XReport
                | DocType::ZReport,
            ShiftState::ClosingLocalPendingDrain,
            _,
        ) => Some(RejectionReason::PostLocalCloseSaleRefused),

        // ShiftClose on OpenedLocalPendingDrain — explicit per §5.7 L2.
        (DocType::ShiftClose, ShiftState::OpenedLocalPendingDrain, _) => {
            Some(RejectionReason::OfflineShiftCloseNotSupported)
        }
        (DocType::ShiftClose, ShiftState::ClosingLocalPendingDrain, _) => {
            Some(RejectionReason::ShiftClosingInFlight)
        }

        // Catch-all for ShiftClose / ZReport against any non-Opened state
        // not covered by the channel-aware arms above.
        (DocType::ShiftClose, _, _) | (DocType::ZReport, _, _) => {
            Some(RejectionReason::ShiftNotOpen { current: shift_state })
        }
    }
}
```

### 3.5 New RejectionReason variants

Extend `RejectionReason` in `services/write_path/types.rs`:
```rust
pub enum RejectionReason {
    // ── existing variants ──

    // ── W14a-2b §3.3 mode guard rewrite ──
    /// Drain in progress (mode = `GoingOnline`); retry after return-online
    /// completes.  Audit shape: `STAGE_ACQUIRE_GOING_ONLINE_REFUSED`.
    NodeGoingOnlineDrainInFlight,
    /// Node mode = `Blocked` (operator manual recovery in flight).
    /// Audit shape: `STAGE_ACQUIRE_BLOCKED_REFUSED`.
    NodeBlocked,
    /// Node mode = `StopMode` (legal hold / regulatory pause).
    /// Audit shape: `STAGE_ACQUIRE_STOP_MODE_REFUSED`.
    NodeStopMode,
    /// Node mode = `CryptoDegraded` (key expiry imminent / sidecar down).
    /// Audit shape: `STAGE_ACQUIRE_CRYPTO_DEGRADED_REFUSED`.
    NodeCryptoDegraded,

    // ── W14a-2b §3.4 channel-aware shift guard ──
    /// W14a-2b §1.5 — online op attempted on OpenedLocalPendingDrain.
    /// Audit shape: `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED`.
    ShiftOpenPendingDrainOpRefused,
    /// W14a-2b §1.5 — any op attempted on ClosingLocalPendingDrain.
    /// Audit shape: `POST_LOCAL_CLOSE_SALE_REFUSED` (per PR #62 §W10).
    PostLocalCloseSaleRefused,
    /// W14a-2b §1.5 — SHIFT_CLOSE on offline-locked shift.
    /// Audit shape: `OFFLINE_SHIFT_CLOSE_NOT_SUPPORTED` (per spec §5.7 L2).
    OfflineShiftCloseNotSupported,
    /// W14a-2b §1.5 — op while ClosingLocalPendingDrain is in-flight.
    /// Audit shape: `SHIFT_CLOSING_IN_FLIGHT`.
    ShiftClosingInFlight,
    /// W14a-2b §3.4 — Offline ZReport on OpenedLocalPendingDrain blocked
    /// until W10/W9b coupled backlog logic exists.  Audit shape:
    /// `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED`.
    ZReportBlockedBacklogDrainPending,
}
```

**Note on `#[non_exhaustive]`** (operator correction): `RejectionReason` is NOT currently `#[non_exhaustive]`.  W14a-2b adds 8 new variants; consider adding `#[non_exhaustive]` in the SAME commit as the variant additions so downstream consumers gain the forward-compat guarantee for future M3b/M3c additions.  **Default**: add `#[non_exhaustive]` (low-cost, prevents future patch-breakage in audit dispatch / metrics code).

### 3.6 Audit events (per refusal)

**Mode guard refusals** (§3.3):

| RejectionReason | Audit event | Severity | Rationale |
|---|---|---|---|
| `NodeGoingOnlineDrainInFlight` | `STAGE_ACQUIRE_GOING_ONLINE_REFUSED` | Info | Expected during return-online drain; transient |
| `NodeBlocked` | `STAGE_ACQUIRE_BLOCKED_REFUSED` | Warning | Operator manual-recovery state — forensic |
| `NodeStopMode` | `STAGE_ACQUIRE_STOP_MODE_REFUSED` | Warning | Regulatory pause; rare |
| `NodeCryptoDegraded` | `STAGE_ACQUIRE_CRYPTO_DEGRADED_REFUSED` | **Critical** | Key/sidecar degradation — operator must intervene before next op succeeds |

**Channel-aware shift guard refusals** (§3.4):

| RejectionReason | Audit event | Severity | Rationale |
|---|---|---|---|
| `ShiftOpenPendingDrainOpRefused` | `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED` | Warning | Operator caller error — wrong channel; expected during drain transition; non-fiscal-committing |
| `PostLocalCloseSaleRefused` | `POST_LOCAL_CLOSE_SALE_REFUSED` | Warning | Post-close lockout per W10; operator must close via drain finalization |
| `OfflineShiftCloseNotSupported` | `OFFLINE_SHIFT_CLOSE_REFUSED` | Warning | §5.7 L2 invariant — offline shift close forbidden by spec |
| `ShiftClosingInFlight` | `SHIFT_CLOSING_IN_FLIGHT_OP_REFUSED` | Warning | Transient — operator should retry after drain completion |
| `ZReportBlockedBacklogDrainPending` | `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED` | Warning | Pre-W10 guardrail; replaced by W10 coupled pool/backlog logic later |

Audit payload across shift-guard refusals: `{"fiscal_number": ..., "shift_id": hex, "doc_type": ..., "current_state": ..., "current_channel": "Online" | "Offline"}`.

Mode-guard refusals payload: `{"fiscal_number": ..., "doc_type": ..., "current_mode": ..., "requested_channel": "would_have_been"}`.

Audit emit location: `stage_acquire::reject(...)` helper — single audit per refused attempt.

### 3.6a Active shift resolution widening (operator correction #3)

`stage_acquire` currently resolves `active_shift: Option<ShiftRow>` ONLY when `node_state.shift_state == ShiftState::Opened`.  With W14a-2b enabling offline fiscal ops in `OpenedLocalPendingDrain`, the resolver MUST cover both states — otherwise allowed offline rows would be inserted with `shift_id = None`, breaking:
- signer_guard (`ShiftMissingForFiscalDoc` returned for legitimate offline ops in `OpenedLocalPendingDrain`)
- downstream `stage_offline_ack` shift-aware path
- audit forensics (orphan fiscal docs without shift attribution)

**Change required** in stage_acquire active-shift resolution logic:

```rust
// Was: only resolve for Opened
let active_shift = if ns.shift_state == ShiftState::Opened { ... } else { None };

// Now: resolve for both states that admit fiscal ops on an open shift row.
let active_shift = if matches!(
    ns.shift_state,
    ShiftState::Opened | ShiftState::OpenedLocalPendingDrain,
) {
    shifts::get_tx(tx, ns.current_shift_id?).await?
} else {
    None
};
```

Reject path (existing `ShiftInvariantViolation` audit) preserved for the case where `shift_state` admits an open shift but `current_shift_id` is None or the shift row can't be read — that's still a structural breach.

### 3.7 stage_offline_ack shift-state widening (§1.5a)

**Operator correction #2** — without this, §3.4 row `(Sell|..., OpenedLocalPendingDrain, Offline) → None` is meaningless: stage_acquire allows the op through, but then dispatcher routes to `stage_offline_ack::run` which currently refuses on `ns.shift_state != ShiftState::Opened`.

Current state (`stage_offline_ack.rs`):
```rust
if ns.shift_state != ShiftState::Opened {
    return Ok(OfflineAckOutcome::Refused(RefusalReason::ShiftNotOpened { current: ns.shift_state }));
}
```

W14a-2b widens the allowed set for regular fiscal docs — **AND** must independently determine doc_type to scope the widening (operator correction #4):

`stage_offline_ack::run` API currently takes only `(pool, doc_id, fiscal_number)`.  The widened-state branch must apply ONLY to regular fiscal docs (Sell / Return / ServiceIn / ServiceOut / CashWithdrawal / XReport).  It MUST NOT rely on stage_acquire having already filtered the doc_type — defence-in-depth invariant (the channel-aware stage_acquire and the offline-ack widening are both load-bearing for correctness; both must independently enforce).

```rust
// W14a-2b §3.7: read doc_type alongside doc state in the existing pre-check.
// fetch_offline_ack_inputs_tx (new helper or widening of existing read) returns
// at minimum: { doc_type, current_state, fiscal_number, shift_id, signed_by_cashier_id }.
let inputs = fd::fetch_offline_ack_inputs_tx(tx, doc_id).await?;
// Cross-FN mismatch guard preserved from W7a.
if inputs.fiscal_number != fiscal_number {
    return Ok(OfflineAckOutcome::Refused(RefusalReason::CrossFnMismatch {
        observed_fiscal_number: inputs.fiscal_number,
    }));
}

// Doc-type scoped shift-state widening.
let shift_state_ok = match inputs.doc_type {
    DocType::Sell | DocType::Return | DocType::ServiceIn | DocType::ServiceOut
        | DocType::CashWithdrawal | DocType::XReport => matches!(
        ns.shift_state,
        ShiftState::Opened | ShiftState::OpenedLocalPendingDrain,
    ),
    // Pattern C SHIFT_OPEN: handled via its own offline-ack path; widened state
    // not applicable here.  ShiftClose / ZReport: never reach stage_offline_ack
    // in W14a-2b because stage_acquire refuses them (§3.4 matrix).
    _ => ns.shift_state == ShiftState::Opened,
};
if !shift_state_ok {
    return Ok(OfflineAckOutcome::Refused(RefusalReason::ShiftNotOpened {
        current: ns.shift_state,
    }));
}
```

**No other stage_offline_ack mutation in W14a-2b.**  The widening is the minimum required to make §3.4's `OpenedLocalPendingDrain + Offline + fiscal op` path actually execute end-to-end.

**Test impact:** existing W7a tests cover `Opened` happy path; W14a-2b adds two new tests:
- `stage_offline_ack_opened_local_pending_drain_accepts_regular_fiscal_doc` — widened branch happy path.
- `stage_offline_ack_opened_local_pending_drain_refuses_non_widened_doc_type` — guards against accidental widening for non-regular-fiscal doc types if any future ingress slipped through.

---

## 4. TransitionOutcome::Conflict test polish (§1.6)

### 4.1 Test design

Add to `rust/prro/tests/shift_state_whitelist_matrix.rs`:

```rust
/// W14a-2b §1.6 — locks shifts::TransitionOutcome::Conflict variant.
/// Currently variant-defined + reachable through integration paths but
/// no dedicated unit test (R7 LOW carry-forward from W14a-2a).
/// Mirrors fiscal_documents test at repo_fiscal_documents_state_cas.rs:117.
#[tokio::test]
async fn transition_state_returns_conflict_when_observed_state_drifted() {
    use prro::db::tx::with_immediate;
    let (pool, fn_id) = fresh_with_fn().await;
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::Opened).await;

    let outcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            // Mutate state to Closing OUTSIDE the whitelist seam via raw
            // UPDATE (simulates concurrent admin path that bypassed the
            // typed transition surface).
            sqlx::query("UPDATE shifts SET state = 'CLOSING' WHERE shift_id = ?")
                .bind(shift_id)
                .execute(&mut **tx)
                .await?;
            // Whitelist-allowed transition (Opened → Closing edge 8) BUT
            // current state at UPDATE will be 'CLOSING', not 'OPENED'.
            // CAS WHERE shift_id = ? AND state = 'OPENED' → 0 rows.
            // Diagnostic re-read returns 'CLOSING' → Conflict { observed: Closing }.
            let o = shifts::transition_state(tx, shift_id, ShiftState::Opened, ShiftState::Closing).await?;
            anyhow::Ok(o)
        })
    }).await.unwrap();

    assert_eq!(
        outcome,
        TransitionOutcome::Conflict { observed: ShiftState::Closing },
        "post-drift transition_state must surface Conflict with observed=Closing",
    );
}
```

### 4.2 Acceptance

Test added.  Total shift_state tests: 593 (W14a-2a baseline 592 + 1 new).  No production code change.

---

## 5. Files changed (estimate)

| File | Change | LoC est. |
|---|---|---|
| `rust/prro/migrations/017_signed_by_cashier_id.sql` | new migration (Rust contour ONLY; root `sql/` untouched) | ~10 |
| `rust/prro/src/db/repositories/fiscal_documents.rs` | `signed_by_cashier_id` on `NewDocument`, `DocumentRow`, `SendInputs` (+`document_id`, `shift_id`); INSERT + fetch_send_inputs_tx + fetch_offline_ack_inputs_tx widened | ~80 |
| `rust/prro/src/db/repositories/shifts.rs` | `ShiftRow` exposes `opened_by_cashier_id: CashierId`; `get` + `get_tx` SELECTs widened (operator correction #1) | ~20 |
| `rust/prro/src/services/write_path/types.rs` | `Channel` enum + 8 new `RejectionReason` variants + `#[non_exhaustive]` + `CanonicalFiscalCommand.signed_by_cashier_id` | ~70 |
| `rust/prro/src/services/write_path/stage_acquire.rs` | mode guard rewrite (Online/Offline/GoingOffline pass; GoingOnline/Blocked/StopMode/CryptoDegraded refuse) + active-shift resolution widening for OpenedLocalPendingDrain (§3.6a, operator correction #3) + `check_shift_guard` signature change + new matrix arms + audit dispatch | ~140 |
| `rust/prro/src/services/write_path/signer_guard.rs` | NEW — `enforce_signer_cashier_match` helper + `SignerCashierMismatch` enum + audit payload builder (so W9b can reuse) | ~100 |
| `rust/prro/src/services/write_path/stage_send.rs` | invoke `enforce_signer_cashier_match` in 4-pre tx BEFORE envelope build / CAS / trace / audit / wire send; new `PreOutcome::SignerRefused(_)` Ok-variant; audit emit on refusal | ~60 |
| `rust/prro/src/services/write_path/stage_offline_ack.rs` | doc_type-scoped shift-state widening — read doc_type via `fetch_offline_ack_inputs_tx`, then allowed set `Opened | OpenedLocalPendingDrain` ONLY for regular fiscal docs (§3.7, operator correction #4) | ~30 |
| Adapters / ingress (Rust-only) | All `CanonicalFiscalCommand { ... }` constructors updated for new field (test fixtures + Maria/REST adapters) | ~40 |
| `rust/prro/tests/shift_state_whitelist_matrix.rs` | Conflict variant test (§1.6) | ~25 |
| `rust/prro/tests/stage_acquire_channel_aware.rs` | NEW — channel × state × doc_type matrix coverage | ~300 |
| `rust/prro/tests/stage_send_signer_cashier_match.rs` | NEW — happy + mismatch + ShiftClose/Z_REPORT bypass + missing id + missing shift paths | ~200 |
| `rust/prro/tests/stage_offline_ack_opened_local_pending_drain.rs` | NEW — covers §3.7 widened branch | ~80 |
| `rust/prro/tests/migration_017_signed_by_cashier_id.rs` | NEW — (a) fresh apply on empty DB adds column with default NULL, (b) upgrade from migration 016 applies cleanly preserving existing rows with NULL backfill, (c) migration runner re-run does NOT reapply recorded migration (no second `ALTER TABLE ADD COLUMN` — would fail; relies on runner's recorded-checksum gate) | ~70 |

**Total est.**: ~935 LoC product + ~675 LoC tests = ~1610 LoC diff.  Within plan §3 (W14a-2b) envelope (~1500-1700 LoC budget).

---

## 6. PRRO invariant verification

| Invariant | Verdict | Evidence |
|---|---|---|
| I1 (no network/crypto in long SQLite write tx) | **preserved** | All new logic is in-tx DB reads/writes; signer match is pure in-memory comparison after WorkerContext load |
| I2 (one FN = one writer) | **preserved** | No change to BEGIN IMMEDIATE serialisation; new check happens in same `with_immediate` |
| I3 (channel switch forbidden with open shift) | **strengthened** | Channel-aware refusals MAKE this explicit at acquire time — currently online-while-OpenedLocalPendingDrain is implicitly refused via `ShiftNotOpen`; W14a-2b emits typed audit |
| I4 (idempotency) | **preserved** | Refusals don't mutate state; retry behavior identical |
| I5 (offline bounded by limits) | **N/A** | This slice doesn't touch offline limits |
| I6 (canonical payload) | **preserved** | CanonicalFiscalCommand extended with field; adapters provide it; no payload shape regression |
| I7 (schema_version) | **preserved** | No envelope schema bump |
| I8 (recovery + state-machine correctness) | **preserved** | Refusal at stage_acquire / stage_send doesn't mutate doc/shift state |
| I9 (graceful shutdown) | **preserved** | Refusals are synchronous; no spawned tasks |
| I10 (minimal diff) | **respected** | Each item self-contained; no cross-cutting refactor |

---

## 7. Acceptance criteria

W14a-2b closes when:

1. ✅ `CanonicalFiscalCommand.signed_by_cashier_id` plumbed end-to-end (ingress → stage 1 → stage 3 → stage 4).
2. ✅ `enforce_signer_cashier_match` helper in `signer_guard.rs` callable from `stage_send` (online) AND structurally ready for `backlog_drain` (W9b) invocation.
3. ✅ `SignerCashierMismatch::{Mismatch, SignerIdMissing}` typed errors + `SIGNER_CASHIER_MISMATCH` Warning audit.
4. ✅ SHIFT_CLOSE / Z_REPORT bypass for §16.9 senior cashier interaction verified by test.
5. ✅ Mode guard rewrite: `Online → Channel::Online`, `Offline | GoingOffline → Channel::Offline`, `GoingOnline | Blocked | StopMode | CryptoDegraded` refused with typed reason + audit BEFORE `check_shift_guard` invocation.
6. ✅ `check_shift_guard` takes `Channel` parameter; matrix covers all 9 shift states × 9 doc types × 2 channels = 162 cases (using full `DocType` variant set; doc-type groupings in the spec table at §3.2 are presentation shorthand — actual code arms expand each branch).  Each case has an explicit verdict (None / Some(Refusal)) — no fall-through reliance.
7. ✅ 9 new `RejectionReason` variants (4 mode-guard + 5 channel-aware) emit corresponding audit events.
8. ✅ `OpenedLocalPendingDrain + offline channel + regular fiscal op` returns `None` (allowed) at stage_acquire AND `stage_offline_ack` accepts the doc (§3.7 widening) — closes W14a-1 defensive arm UX regression end-to-end.
9. ✅ `ClosingLocalPendingDrain + any op + any channel` returns `PostLocalCloseSaleRefused` — preserves PR #62 §W10 lockout contract.
10. ✅ `(ZReport, OpenedLocalPendingDrain, Offline)` returns `ZReportBlockedBacklogDrainPending` (NOT allowed) — pre-W10 guardrail (operator correction #3).
11. ✅ Migration `rust/prro/migrations/017_signed_by_cashier_id.sql` applies cleanly + idempotent re-apply.
12. ✅ TransitionOutcome::Conflict test in `shift_state_whitelist_matrix.rs` passes (§1.6).
13. ✅ Full test suite (`cargo test -p prro --features test-support`) green at 595+ / 0 / 1 (W14a-2a baseline 592 + Conflict + stage_offline_ack widened + signer mismatch suites).
14. ✅ Clippy clean: `cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings`.
15. ✅ Senior review pass (operator-triggered `проверь W14a-2b`) closes any HIGH/MED findings before merge.

---

## 8. Open questions — RESOLVED 2026-05-19 (operator)

1. **CanonicalFiscalCommand backward compatibility** — **RESOLVED**.  Operator confirms `CanonicalFiscalCommand` is not currently `Deserialize`.  Breakage scope: Rust struct literals / test fixtures only.  Adding `signed_by_cashier_id: Option<CashierId>` is okay; update all Rust constructors.  No serde concern unless new code adds serde.

2. **Migration ordering** — **RESOLVED**.  Use `rust/prro/migrations/017_signed_by_cashier_id.sql`.  Ignore root `sql/017_*` (Python/historical contour; out of M3b Rust scope).

3. **Channel enum location** — **RESOLVED**.  Place in `services/write_path/types.rs`.  Do NOT put in `db/models/enums.rs` (not persisted DB vocabulary).

4. **Offline ZReport edge** — **RESOLVED**.  Block in W14a-2b via `ZReportBlockedBacklogDrainPending` refusal + `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED` audit.  Do NOT allow until W10/W9b coupled logic exists.  W10 later replaces this refusal with the proper coupled pool/backlog/edge-7 guard.

5. **W14a-2b PR splitting** — **RESOLVED**.  One PR acceptable after spec fixes.  No need to split signer/channel unless diff grows unexpectedly during implementation (operator may re-evaluate post-implementation).

---

## 9. Out of scope (deferred)

- **W9b** — offline backlog drain orchestration (uses `enforce_signer_cashier_match` helper hoisted in W14a-2b).
- **W10** — Z-report guard while offline backlog non-empty.  W14a-2b adds pre-W10 guardrail via `ZReportBlockedBacklogDrainPending`; W10 replaces with proper coupled pool/backlog/edge-7 logic.
- **W14a-3** — multi-cashier shift role registry; SHIFT_CLOSE/Z_REPORT senior-cashier role policy beyond §16.9 close seam.
- **Offline-time signer warning audit** (§2.7 OQ) — flag at offline-ack time vs at drain time.  Default: drain-time only.
- **Root `sql/` Python migrations + `src/prro_gateway/` Python source** — historical contour; not in M3b Rust scope at all.  All persistence DDL for W14a-2b lives under `rust/prro/migrations/`.

---

## 10. Implementation sequencing (operator-recommended)

Recommended commit chain within the single W14a-2b PR:

1. **Commit 1 — Migration + repo plumbing**
   - `rust/prro/migrations/017_signed_by_cashier_id.sql`
   - `NewDocument` / `DocumentRow` gain `signed_by_cashier_id: Option<CashierId>`
   - **`SendInputs` explicitly gains `document_id: DocumentId` + `shift_id: Option<ShiftId>` + `signed_by_cashier_id: Option<CashierId>`** (operator correction #2 — fields NOT currently present; firm requirement, no "likely already present")
   - **`ShiftRow` exposes `opened_by_cashier_id: CashierId`** + `shifts::get` / `shifts::get_tx` SELECTs widened (operator correction #1 — was PR #66 L3 deferred, now in-scope)
   - `insert_prepared` / `insert_prepared_tx` / `fetch_send_inputs_tx` / new `fetch_offline_ack_inputs_tx` / all `get_*` selectors updated
   - `stage_acquire` `NewDocument` construction updated for new field
   - Migration test added (§5 catalog) covering: fresh apply / upgrade-from-016 / runner-recorded re-run (no second ALTER)

2. **Commit 2 — `CanonicalFiscalCommand.signed_by_cashier_id`**
   - Field added to struct
   - All Rust constructors / test fixtures / Maria + REST adapters updated
   - No serde concern (operator-verified)

3. **Commit 3 — Signer guard helper**
   - NEW `services/write_path/signer_guard.rs`
   - `SignerCashierMismatch` enum (`Mismatch` / `SignerIdMissing` / `ShiftMissingForFiscalDoc`)
   - `enforce_signer_cashier_match(inputs: &SendInputs, shift: Option<&ShiftRow>) -> Result<(), _>`
   - Bypass for `ShiftClose | ZReport` per §16.9
   - Helper unit tests

4. **Commit 4 — Channel-aware stage_acquire**
   - `Channel` enum in `services/write_path/types.rs`
   - 9 new `RejectionReason` variants + `#[non_exhaustive]`
   - Mode guard rewrite (Online/Offline/GoingOffline pass; others refuse with typed reason)
   - `check_shift_guard(doc_type, shift_state, channel)` signature change
   - New matrix arms (incl. `ZReportBlockedBacklogDrainPending` for pre-W10 guardrail)
   - Audit dispatch for all new refusal variants in `reject(...)`

5. **Commit 5 — Stage_send signer integration**
   - `stage_send::run` 4-pre tx calls `enforce_signer_cashier_match` BEFORE envelope build / CAS / trace / audit / wire send
   - New `PreOutcome::SignerRefused(SignerCashierMismatch)` Ok-variant
   - Audit emit (`SIGNER_CASHIER_MISMATCH` Warning) inside same `with_immediate` tx, BEFORE return Ok
   - Integration test

6. **Commit 6 — Stage_offline_ack widening**
   - Shift-state allowed set: `Opened | OpenedLocalPendingDrain` for regular fiscal docs
   - Integration test for OpenedLocalPendingDrain offline-ack acceptance

7. **Commit 7 — Tests + TransitionOutcome::Conflict polish**
   - `tests/shift_state_whitelist_matrix.rs` Conflict variant test (§1.6)
   - `tests/migration_017_signed_by_cashier_id.rs` apply + idempotent re-apply
   - `tests/stage_acquire_channel_aware.rs` matrix coverage
   - Any remaining integration tests for cross-stage signer flow

Each commit independently compiles + tests green.  Review can step through the chain.

---

## 11. Worktree + branch convention

- Branch: `m3b/w14a-2b-signer-channel` (off `rust-gateway` `67add6b`).
- Worktree: `/mnt/d/PRRO_GATE-m3b-w14a-2b/`.
- PR target: `rust-gateway`.
- Merge style: `gh pr merge --merge` (per operator's PR merge style memory — NOT `--squash`).
