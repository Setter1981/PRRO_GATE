# M3a W10 — DpsError Routing Dispatch (full 8-variant + 12-status-code table) — Design Freeze

**Date:** 2026-05-10
**Status:** v2 — 4 blockers + Q1-Q5 finalised; ready for W10.1 apply
**Anchors:** ADR-M3-A6 (full retry policy table), ADR-M3-A9 step 5-6 (Pattern B retry path), W0-3 §2 main + §2.1 sub-table + §9.2 acceptance, M3a plan Task 9
**Predecessor:** W8 (PR #27 + #28, merged `1d29315`) — stage 5 finalize
**Successor:** W9 (App::boot reconciliation; consumes W10's routing in non-live context), W11 (deterministic-replay gate)

---

## 1. Purpose & scope

W10 lands the **complete `DpsError` → `DocState` routing contract** that W7 stage 4 send opted out of (W7-conservative shipped `classify_send_outcome` mapping all `Server { code }` to `Retryable::Server`).  Every wire reply now resolves to an explicit, testable routing decision per W0-3 §2 + §2.1.

**Three concrete deliverables:**
1. **Pure-fn routing module** `services/write_path/error_routing.rs` — `route_dps_error(err, doc_type, is_live_send) → RoutingDecision`.  No DB, no I/O.
2. **Stage 4 wire-in** — `stage_send.rs` replaces minimal `classify_send_outcome` with the new routing call; existing `SendOutcome` enum extended to carry the routing decision shape.
3. **MAC recovery -12 in-stage path** — regex-extract `store {64hex}`, one bounded re-derive + re-sign + re-send via Pattern B (per ADR-M3-A6 §2.1 row -12).

**Two side effects W10 must wire:**
- `node_state.mode → BLOCKED` on `Server { code: -11 }` (cumulative-offline-168h breach; per W0-3 §2.1 + W0-1 §2.4).
- `last_chk` reconciliation **hint** (deferred execution) for Decode (status=0), `-2` close-shift, `-15` close-shift — W10 routes the doc to `ErrorRetryable` and emits a `probe_required` audit field; W9 reconciliation worker performs the actual `last_chk` probe.

**21 routing fixtures + 1 MAC recovery fixture** per W0-3 §9.2 — the structural proof that ADR-M3-A6 is enforced behaviour, not documentation.

---

## 2. Anchor matrix

| Anchor | Source : line | Constraint |
|---|---|---|
| **ADR-M3-A6** | `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md:612-647` | W0-3 §2 main + §2.1 sub-table is the binding routing contract.  Three pillars: WebCheck-derived retry classes, per-call gRPC deadline, bounded recovery (default 5). |
| **ADR-M3-A9 step 5-6** | `2026-05-04-m2-pre-plan-adr.md:684-711` | Pattern B retry path: `ErrorRetryable → Sending → wire → Sending → {Sent|Kvt1|Rejected|ErrorRetryable}`.  M3a DPS code MUST NOT use legacy `(ErrorRetryable, Sent)` whitelist `:99` for wire send. |
| **W0-3 §2 main table** | `2026-05-06-m3-w0-3-retry-recovery.md:194-298` | 8 DpsError variants × retry policy + max attempts + backoff + dead-letter + source-state implications. |
| **W0-3 §2.1 sub-table** | `2026-05-06-m3-w0-3-retry-recovery.md:300-348` | 12 Server-routed status codes (the 4 dto-pre-routed codes -1/-4/-13/-14 are covered in §2 main row). |
| **W0-3 §9.2 acceptance** | `2026-05-06-m3-w0-3-retry-recovery.md:1206-1262` | 21 fixtures: 10 §2 main + 11 §2.1 (with `-2`/`-15` two-variant + `-7..-10` parametrised XML-class). |
| **W7 baseline** | `rust/prro/src/services/write_path/stage_send.rs:284-380` | Minimal `classify_send_outcome` shipped W7-conservative — replaced by W10. |
| **DpsError shape** | `rust/prro/src/transports/dps/error.rs:14-93` | 8 variants + `AuthorizationKind` (M2/W4-landed). |

---

## 3. WireDecision + RoutingDecision shape

**Q3 finalised:** `route_send_result(Result<CheckAck, DpsError>, doc_type, is_live_send) → WireDecision`.  The OK arm carries `CheckAck.id` (server_fiscal_no) which can't be expressed via `RoutingDecision` shape; an outer `WireDecision` wrapper preserves it without losing the typed routing for the Err arm.

```rust
/// Pure-fn output of the W10 routing surface.  Wraps the OK arm
/// (which carries `CheckAck.id` for the success path) and the Err
/// arm (which produces a typed RoutingDecision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireDecision {
    /// `Ok(CheckAck)` from `send_chk` — happy path.  4-b CAS
    /// `Sending → Sent`; `set_server_fiscal_no_tx(server_fiscal_no)`;
    /// audit `STAGE_SEND_RESULT` with outcome=OK.
    Sent { server_fiscal_no: String },
    /// `Err(DpsError)` from `send_chk` — typed routing per
    /// W0-3 §2 + §2.1.  4-b CAS `Sending → decision.target_state`;
    /// dispatch side effects per `decision`.
    Routed(RoutingDecision),
}

/// Pure-fn routing of a `DpsError` into a `(target_state, retry_class,
/// audit, side-effect-hints)` decision.  No DB, no clock, no I/O.
/// Stage 4 (live, is_live_send=true) consumes this; W9 reconciliation
/// will consume the same shape with is_live_send=false.
///
/// **Invariant (B2 close):** `target_state` is ALWAYS `DocState` —
/// every live DpsError finalises 4-b in an explicit state.  No
/// "no DocState transition" branch exists at this layer; W0-3 §2
/// rows that say "no transition" (NotFound, QueryNotSupported)
/// surface here as `WrapperBug` retry class with `target_state =
/// ErrorRetryable` and CRITICAL audit — leaving SENDING durable
/// without a transition would create a recovery-sensitive stuck
/// state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingDecision {
    /// Target DocState after the post-CAS commit.  ALWAYS `DocState`
    /// (no AbortLivePostWire variant).  Per B2 fold:
    /// - Live (is_live_send=true): from `Sending` source.
    /// - Reconciliation (is_live_send=false): from `Sent` source (W9 territory).
    pub target_state: DocState,

    /// Class for retry/backoff bookkeeping + audit severity.
    pub retry_class: RetryClass,

    /// Audit event_type to append on the post-CAS commit (rich payload
    /// composed by the caller).  Closed enum (Q4 close) — every
    /// audit event_type ever emitted by W10 is enumerated here.
    pub audit_event: AuditEvent,

    /// Audit severity (INFO / WARN / ERROR / CRITICAL).
    pub audit_severity: Severity,

    /// Side-effect: flip `node_state.mode` to `BLOCKED` (only for
    /// `Server { code: -11 }`; None otherwise).
    pub node_mode_flip: Option<NodeMode>,

    /// Side-effect: schedule a `last_chk` reconciliation probe (W9
    /// territory).  `Some` for Decode (status=0), `-2` close-shift,
    /// `-15` close-shift.  W10 emits this in the audit payload as
    /// `probe_required = true`; W9 picks up.
    pub probe_hint: Option<ProbeHint>,

    /// Side-effect: invoke the bounded MAC recovery path (only for
    /// `Server { code: -12 }`).  W10 acts on this synchronously in
    /// stage_send::run.
    pub mac_recovery_hint: Option<MacRecoveryHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    /// Terminal — no retry, no recovery.  Routed to `Rejected`.
    /// Applies to: Authorization{DocumentReject},
    /// Server{-2 non-shift,-5,-7..-10,-11,-16}, Server{-15} non-shift,
    /// Server{-12} hash-not-extractable fallback.
    TerminalReject,
    /// Transient — re-driven via Pattern B `(ErrorRetryable → Sending → wire)`.
    /// Bounded by `max_recovery_attempts=5` (default).
    /// Applies to: Transport, Server{-3}, Server{-4} reroute via Transport.
    TransientRetry,
    /// Authorization sub-class for FN-config errors (-13 / -14).
    /// Routed to `ErrorRetryable` with audit `STAGE_SEND_FN_NOT_REGISTERED`;
    /// W9 chains `ErrorRetryable → RequiresManualReconciliation`.
    FnConfigError,
    /// Wrapper-side bug or invariant breach
    /// (Internal, ServerFiscalIdMismatch, NotFound on live send,
    /// QueryNotSupported on live send, unknown Server code).
    /// Routed to `ErrorRetryable` with CRITICAL audit; W9 chains to
    /// `RequiresManualReconciliation`.  NotFound and QueryNotSupported
    /// folded into this class per B2: live send_chk should never
    /// produce these, but if it does, doc must NOT be left durably
    /// in SENDING.
    WrapperBug,
    /// Decode / `-2` close-shift / `-15` close-shift — needs a
    /// `last_chk` probe to disambiguate.  W10 routes to ErrorRetryable
    /// + emits probe_hint; W9 performs the probe.
    ProbeRequired,
    /// Server `-12` ERROR_BAD_HASH_PREV — bounded ONE auto-recovery
    /// (regex-extract + re-derive + re-sign + re-send via Pattern B).
    /// W10 acts synchronously; on success → Sent; on failure → Rejected.
    MacRecovery,
    /// `Server{-6}` ERROR_NOT_PREV_ZREPORT — operator-recoverable,
    /// not auto-retried.  Routed via ErrorRetryable →
    /// RequiresManualReconciliation chain (W9 step).
    OperatorEscalation,
}

/// Closed enum of every audit event_type W10 may emit.  As-str
/// strings are the canonical wire form; written into
/// `audit_log.event_type` TEXT column.  Adding a new event requires
/// extending this enum AND a fixture asserting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEvent {
    StageSendResult,                        // happy + transient retry
    StageSendRejected,                      // terminal reject (-1, -2, -5..-10, -15 non-shift, -16, -12 fallback)
    StageSendFnNotRegistered,               // -13 / -14
    StageSendWrapperBug,                    // Internal / NotFound (live) / QueryNotSupported (live)
    StageSendFiscalIdMismatch,              // ServerFiscalIdMismatch (CRITICAL)
    StageSendDecodeUnknown,                 // status=0; probe required
    StageSendProbeRequired,                 // -2/-15 close-shift; probe required
    StageSendNodeBlocked,                   // -11; node_state.mode → BLOCKED
    StageSendOperatorEscalation,            // -6
    MacRecoveryHashNotExtractable,          // -12 with malformed message
    MacRecoveryResigned,                    // -12 successful re-pin + re-sign
    MacRecoveryFailedRepeatHashMismatch,    // -12 → -12 second time
}

impl AuditEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StageSendResult => "STAGE_SEND_RESULT",
            Self::StageSendRejected => "STAGE_SEND_REJECTED",
            Self::StageSendFnNotRegistered => "STAGE_SEND_FN_NOT_REGISTERED",
            Self::StageSendWrapperBug => "STAGE_SEND_WRAPPER_BUG",
            Self::StageSendFiscalIdMismatch => "STAGE_SEND_FISCAL_ID_MISMATCH",
            Self::StageSendDecodeUnknown => "STAGE_SEND_DECODE_UNKNOWN",
            Self::StageSendProbeRequired => "STAGE_SEND_PROBE_REQUIRED",
            Self::StageSendNodeBlocked => "STAGE_SEND_NODE_BLOCKED",
            Self::StageSendOperatorEscalation => "STAGE_SEND_OPERATOR_ESCALATION",
            Self::MacRecoveryHashNotExtractable => "MAC_RECOVERY_HASH_NOT_EXTRACTABLE",
            Self::MacRecoveryResigned => "MAC_RECOVERY_RESIGNED",
            Self::MacRecoveryFailedRepeatHashMismatch => "MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeHint {
    /// Why W9 should probe (carried in audit payload for forensics).
    pub reason: ProbeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeReason {
    /// status=0 UNKNOWN — proto-default, server contract drift suspected.
    DecodeUnknown,
    /// `-2` ERROR_CHECK + doc_type ∈ {SHIFT_CLOSE, Z_REPORT}.
    Code2CloseShift,
    /// `-15` ERROR_NOT_OPEN_SHIFT + doc_type ∈ {SHIFT_CLOSE, Z_REPORT}.
    Code15CloseShift,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacRecoveryHint {
    /// Original wire error message; W10 regex-extracts the expected
    /// `store {64hex}` from it.  If extraction fails → MacRecovery
    /// downgrades to TerminalReject + audit `MacRecoveryHashNotExtractable`.
    pub raw_error_message: String,
}
```

**Why a struct + outer enum:** `WireDecision` enum captures the OK/Err split (CheckAck.id is only on OK).  `RoutingDecision` struct carries the multi-faceted Err shape (target state, retry class, audit, side-effect hints) — an enum would lose information or duplicate variants combinatorially.

---

## 4. Decomposition (5 sub-units)

### 4.1 W10.1 — `error_routing.rs` pure-fn module
- New `rust/prro/src/services/write_path/error_routing.rs`:
  - `pub enum WireDecision`, `pub struct RoutingDecision`, `pub enum RetryClass`, `pub enum AuditEvent` (closed) + `as_str()`, `pub struct ProbeHint`, `pub enum ProbeReason`, `pub struct MacRecoveryHint`.
  - `pub fn route_send_result(r: Result<CheckAck, DpsError>, doc_type: DocType, is_live_send: bool) -> WireDecision` — outer wrapper preserving `CheckAck.id`.
  - `pub fn route_dps_error(err: &DpsError, doc_type: DocType, is_live_send: bool) -> RoutingDecision` — pure-fn the Err arm dispatches to.
  - **Exhaustive `match err` (B1 close).**  `DpsError` is NOT `#[non_exhaustive]` (verified at `transports/dps/error.rs:15`).  Match has explicit arms for ALL 8 variants: Transport / Authorization{kind:DocumentReject} / Authorization{kind:FiscalNumberNotRegistered} / Decode / Server / NotFound / ServerFiscalIdMismatch / QueryNotSupported / Internal — **no `_` catch-all**.  If a future commit adds a 9th DpsError variant, the build BREAKS — exactly the safety net we want; the match-arm forces explicit routing decision rather than silently default-routing.
  - **Server{code:i32} fail-closed default arm (B1 + R-W10-1 close).**  For the `Server { code, message }` arm, dispatch to a private `route_server_code(code, message, doc_type, is_live_send)`.  Inside, match the **12 known codes** (-2/-3/-5/-6/-7..-10/-11/-12/-15/-16) explicitly; trailing `_` arm for **unknown** codes routes to `WrapperBug` retry class with `target_state = ErrorRetryable`, `audit_event = StageSendWrapperBug`, `audit_severity = Critical`.  `i32` is not an enum, so the catch-all is required here (B1 distinction: enum match exhaustiveness vs raw int dispatch).
  - **Live-only routing for query variants (B2 close).**  `NotFound` / `QueryNotSupported` on `is_live_send=true` route to `WrapperBug` → `target_state = ErrorRetryable` + CRITICAL audit `StageSendWrapperBug`.  These shapes should NEVER come from `send_chk` in live; the WrapperBug routing ensures the doc doesn't get stuck in SENDING.  Reconciliation context (`is_live_send=false`) preserves the original "no DocState transition" semantic for these — that's W9's contract.
  - Unit tests inside the file (`mod tests`): 21 cases (10 main + 11 sub-table) + 4 RetryClass sanity checks + 1 unknown-Server-code fail-closed test.  Pure Rust — no DB, no async, sub-millisecond.
- Register `pub mod error_routing;` in `services/write_path/mod.rs`.

### 4.2 W10.2 — Wire routing into `stage_send.rs`
- Replace `classify_send_outcome` body with:
  ```rust
  let decision = error_routing::route_dps_error(&err, inputs.doc_type, /* is_live_send */ true);
  ```
  for the `Err(_)` arm; keep the `Ok(ack)` happy path unchanged.
- Extend `SendOutcome` enum with a new variant `Routed { decision: RoutingDecision }` OR fold directly: replace `classify_send_outcome → SendOutcome` with `→ RoutingDecision` and update 4-b commit logic to dispatch on `decision.target_state`.  **Recommend the latter** — fewer indirections; W7's `SendOutcome` shape was always meant to be replaced (per W7 freeze §6).
- 4-b commit applies `decision.target_state` via CAS; emits audit `decision.audit_event` with rich payload (existing payload + `retry_class`, `probe_required: bool`, `node_mode_flipped: bool`).
- W3 static scan stays green (no foreign IO inserted into closures).

### 4.3 W10.3 — `node_state.mode → BLOCKED` side effect for `-11`
- Add `pub async fn set_mode_blocked_tx(tx: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<bool>` to `node_state.rs`.  Mirror of existing tx-bound update helpers; returns `bool` for missing-row detection per W7.2 / W8.2 convention.
- In `stage_send.rs` 4-b closure: if `decision.node_mode_flip == Some(NodeMode::Blocked)`, invoke the helper inside the same `with_immediate` envelope as the post-CAS write.  Atomic with the doc-state transition.
- Audit payload extended: `node_mode_flipped: "BLOCKED"` recorded.

### 4.4 W10.4 — MAC recovery `-12` in-stage path

**Q1 finalised: dedicated `mac_recovery_attempts` column + migration 012.**  `recovery_attempts` does not exist in the current schema (verified) — reuse was never an option.  Add a single-bit budget column with a CHECK so the bounded-ONE invariant is DDL-enforced.

**Migration 012:**
```sql
-- migrations/012_mac_recovery_attempts.sql
ALTER TABLE fiscal_documents
  ADD COLUMN mac_recovery_attempts INTEGER NOT NULL DEFAULT 0
  CHECK (mac_recovery_attempts IN (0, 1));
```

**B4 close — separate `mac_recovery_repin_tx` helper, NOT W6 pin reuse.**  W6 pin reads `node_state.last_known_unsigned_xml_sha256`; MAC recovery uses the DPS-extracted hash from the error_message.  Reusing W6 pin would re-read node_state and overwrite our extracted-hash with the chain seed — wrong.  New helper:

```rust
// fiscal_documents.rs — MAC recovery re-pin (distinct from W6 pin)
pub async fn mac_recovery_repin_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    extracted_hash: &[u8; 32],
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE fiscal_documents SET \
            previous_hash         = ?, \
            mac_recovery_attempts = mac_recovery_attempts + 1 \
         WHERE document_id = ? \
           AND state = 'SENDING' \
           AND mac_recovery_attempts = 0",
    )
    .bind(&extracted_hash[..])
    .bind(doc_id)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected())
}
```
- Source-state guard: `state='SENDING'` (the doc must be in SENDING from 4-pre, NOT PREPARED — that's W6's source).
- Counter guard: `mac_recovery_attempts = 0` (first and only attempt).
- `signing_inputs_pinned_at` is intentionally NOT reset — the row is still pinned, just with a recovered `previous_hash`.

**Q2 finalised — re-sign helper extraction (rebuild + sign, no Z alloc, no Prepared→Signed CAS).**  Extract from `stage_sign.rs` a new helper that does the no-tx 3-NO-TX segment:

```rust
// stage_sign.rs — extracted re-sign helper
pub async fn re_sign_after_mac_recovery(
    crypto: Arc<dyn CryptoProvider>,
    session: SigningSession,
    profile: CmsProfile,
    inputs: PinnedSigningInputs,  // includes the recovered previous_hash
    typed_payload: TypedPayload,
    header: DocumentHeader,
    local_number: u32,
    wire_artifact_kind: WireArtifactKind,
) -> Result<RebuildedAndSigned, SignError> {
    // No-tx, no Z alloc, no Prepared→Signed CAS.
    // 1. build_canonical_xml(wire_artifact_kind, header, local_number, typed_payload).
    // 2. sha256(unsigned_xml).
    // 3. sign_cms_detached(provider, session, profile, unsigned_xml).
    // Returns the rebuilt unsigned_xml + sha256 + signed_payload bytes.
}
```
This is invoked OUTSIDE any `with_immediate` per W3 invariant.

**B3 close — atomic multi-write at MAC recovery PERSIST step.**  After re-sign, atomically replace ALL THREE artifacts inside one `with_immediate` (so the next-doc chain advance W8 picks up sees a consistent triple):

```rust
// fiscal_documents.rs — MAC recovery PERSIST atomic helper
pub async fn mac_recovery_persist_resigned_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    new_unsigned_xml_sha256: &[u8; 32],
    new_payload_xml: &[u8],
    new_signed_xml: &[u8],
) -> sqlx::Result<()> {
    // 1. UPDATE fiscal_documents.unsigned_xml_sha256 = new sha.
    // 2. UPDATE document_files SET content = new_payload_xml WHERE kind = 'PAYLOAD_XML'.
    // 3. UPDATE document_files SET content = new_signed_xml  WHERE kind = 'SIGNED_XML'.
    // All three under one BEGIN IMMEDIATE; if any fails, rollback all.
    // (B3 close: previous_hash + unsigned_xml_sha256 + PAYLOAD_XML +
    // SIGNED_XML must NEVER drift; W8 stage 5 reads
    // unsigned_xml_sha256 to advance the chain seed — stale value
    // freezes the chain.)
}
```

**Orchestrator (`mac_recovery.rs`):**
```rust
pub(super) async fn run_mac_recovery(
    pool: &SqlitePool,
    crypto: Arc<dyn CryptoProvider>,
    session: SigningSession,
    profile: CmsProfile,
    dps_channel: &dyn DpsChannel,
    doc: DocumentId,
    hint: &MacRecoveryHint,
) -> Result<MacRecoveryOutcome, StageSendError>
```

Steps (per ADR-M3-A6 §2.1 row -12 + Python `dps_fiscal_server.py:494` + `write_path.py:903-994`):

1. **Hash extraction (pure-fn):** `regex_extract_store_hash(&hint.raw_error_message) → Option<[u8; 32]>`.  Pattern: `r"store ([0-9a-fA-F]{64})"`.  Failure → `MacRecoveryOutcome::HashNotExtractable` → caller routes to TerminalReject + audit `MacRecoveryHashNotExtractable`.
2. **Re-pin (with_immediate #1):** `mac_recovery_repin_tx(tx, doc, extracted_hash)`.  Returns `rows_affected`; `0` means either state≠SENDING OR mac_recovery_attempts>0 → caller routes to TerminalReject + audit (counter exhausted).
3. **Re-sign (no-tx):** invoke `re_sign_after_mac_recovery(...)` → rebuilt unsigned_xml + new sha256 + new signed_payload bytes.  W3 invariant satisfied (sign outside any tx).
4. **PERSIST (with_immediate #2):** `mac_recovery_persist_resigned_tx(tx, doc, &new_sha, &new_payload_xml, &new_signed_xml)` — atomic triple write per B3.  Audit `MacRecoveryResigned` with payload `{old_previous_hash, new_previous_hash, new_unsigned_xml_sha256_hex}` for forensic correlation.
5. **Re-send (no-tx + Pattern B):** call `dps_channel.send_chk(new_envelope).await` OUTSIDE locks.  Result returned as `MacRecoveryOutcome::ResignSucceeded { fresh_send_outcome: WireDecision }` — caller then runs the standard 4-b dispatch on the fresh outcome.
6. **Repeat -12 detection:** if the fresh outcome is again `Server { code: -12 }`, the routing fn (W10.1) sees `mac_recovery_attempts == 1` (DDL CHECK enforces no second attempt anyway) and routes to TerminalReject + audit `MacRecoveryFailedRepeatHashMismatch`.

**Doc-files reads:** to re-build the canonical XML (step 3), `re_sign_after_mac_recovery` needs the typed payload + header + local_number.  These can be reconstructed from `fiscal_documents.payload_json` + the recovered `previous_hash` + lnd.  The typed payload parser (`parse_payload`) already exists in W6 stage_sign.

**LoC budget:** ~250 in mac_recovery.rs, ~30 in fiscal_documents.rs (2 new helpers), ~30 in stage_sign.rs (extracted re-sign helper), ~40 integration in stage_send.rs.  Migration 012 = 5 LoC SQL + 1 schema fixture (~30 LoC).

### 4.5 W10.5 — Test fixtures
- New `rust/prro/tests/write_path_dps_error_routing.rs`: 21 fixtures per W0-3 §9.2 (lines 1218-1256).  Pure-DB integration via stub `DpsChannel` (mirror W7.5 stub pattern); driver invokes `stage_send::run` end-to-end; asserts post-tx `state` + audit event_type + audit payload `retry_class` + (where applicable) `node_state.mode` + `probe_hint`.
  - 1 Transport
  - 2 Authorization{DocumentReject, -1}
  - 3-4 Authorization{FiscalNumberNotRegistered, -13/-14}
  - 5 Decode (status=0) → ErrorRetryable + probe_hint=DecodeUnknown
  - 6 NotFound (out-of-band; no state mutation)
  - 7 ServerFiscalIdMismatch
  - 8 QueryNotSupported (out-of-band; no state mutation)
  - 9 Internal
  - 10 Server{-2} non-shift → Rejected
  - 11 Server{-2} close-shift → ErrorRetryable + probe_hint=Code2CloseShift
  - 12 Server{-3} → ErrorRetryable
  - 13 Server{-5} → Rejected
  - 14 Server{-6} → ErrorRetryable (operator escalation hint)
  - 15 Server{-7..-10} parametrised → Rejected
  - 16 Server{-11} → Rejected + node_mode=BLOCKED
  - 17 Server{-12} → covered by W10.4 separate fixture
  - 18 Server{-15} non-shift → Rejected
  - 19 Server{-15} close-shift → ErrorRetryable + probe_hint=Code15CloseShift
  - 20 Server{-16} M3a → Rejected (alert audit)
  - 21 Pattern B retry-path proof — verifies M3a never invokes `(ErrorRetryable, Sent)` whitelist `:99` for wire send (provider spy on a Transport-class fixture; assert spy observed exactly one CAS path through Sending).
- New `rust/prro/tests/write_path_mac_recovery.rs`:
  - Happy: `Server{-12}` with extractable hash → re-sign succeeds → Sent.
  - Hash not extractable: `Server{-12}` with malformed message → `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` + Rejected.
  - Repeat -12 on resend: bounded one attempt → second `-12` → Rejected with `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH`.

---

## 5. Invariants asserted by W10

| # | Invariant | How asserted |
|---|---|---|
| I1 | No network/crypto inside `with_immediate` | `route_dps_error` is pure-fn; MAC recovery re-sign is W6 Pattern A (sign no-tx); W3 static scan stays green |
| I4 | Idempotency mandatory | Every routing decision is deterministic on `(err, doc_type, is_live_send)` — same input → same RoutingDecision → same DocState target |
| I8 | Recovery does not violate state transitions | Pattern B retry path enforced (`ErrorRetryable → Sending → wire`); legacy `(ErrorRetryable, Sent)` whitelist `:99` is NEVER invoked by M3a DPS — fixture 21 proves via provider spy |
| **NEW** | All 8 DpsError variants have explicit routing | `route_dps_error` match arms cover all 8; `#[non_exhaustive]` on DpsError handled via fail-closed default arm + assertion in unit tests |
| **NEW** | All 12 Server-routed codes have explicit routing | `route_server_code` covers all 12; unknown code → `Server::TerminalReject` + CRITICAL audit (fail-closed) |
| **NEW** | MAC recovery is bounded to ONE attempt | Recovery counter check + repeat-on-12 fixture |
| **NEW** | NodeMode flip for -11 atomic with state transition | -11 fixture asserts `node_state.mode == 'BLOCKED'` AND doc state == 'REJECTED' post-tx |

---

## 6. Out of scope (intentionally deferred)

- **`last_chk` reconciliation probe execution.**  W10 emits `probe_hint` in audit payload; W9 reconciliation worker performs the actual `last_chk` probe and decides "drive forward" vs "RequiresManualReconciliation".  Rationale: `last_chk` probes are reconciliation-side work (multi-RPC, asynchronous); embedding them in stage 4 live path would inflate the wire-window and complicate W3 invariant.
- **`is_live_send=false` (reconciliation context).**  W10 wires the routing fn for both contexts but only consumes `is_live_send=true` (live stage 4).  W9 will be the first consumer of the FALSE branch.
- **`Sent → Kvt1 → Kvt2` quittance pipeline.**  Out of scope for both W10 and W9; needs a separate slice (post-M3a or W11-adjacent).
- **WebCheck status `0` precise routing.**  W0-3 §2.1 row 0 deviates from Python ("M3 fails loudly on protocol drift").  W10 enforces this: status=0 → Decode → ProbeRequired (probe_hint=DecodeUnknown).
- **Backoff jitter ±20%.**  W0-3 §2 mentions thundering-herd avoidance; W10 ships fixed exponential backoff for retry-class entries.  Jitter is a recovery-worker concern (W9 retry tick); W10 just records `retry_class`.
- **OFFLINE_LOCAL_ACK / offline-pool reconciliation.**  M3b deferred per CLAUDE.md.  `Server{-16}` in M3a is terminal Rejected; M3b will route to offline-id reconciliation.

---

## 7. Open questions — FINALISED

1. **Q1 — MAC recovery counter (closed: A).**  Dedicated `mac_recovery_attempts` column via **migration 012** with `CHECK (mac_recovery_attempts IN (0, 1))` — DDL-enforced single-bit budget.  `recovery_attempts` does not exist in the current schema (verified), so reuse was never an option.  Tx helper `mac_recovery_repin_tx` carries `WHERE state='SENDING' AND mac_recovery_attempts = 0`; `rows_affected == 0` ⇒ counter exhausted OR wrong state ⇒ TerminalReject + audit.  See §4.4.
2. **Q2 — Re-sign helper (closed: A, scoped).**  Extract `pub async fn re_sign_after_mac_recovery(...)` from `stage_sign.rs`.  **Scope:** rebuild canonical XML + sign already-pinned/recovered inputs.  **Excludes:** new Z-report number allocation; `Prepared → Signed` CAS.  Pure no-tx segment per W3 invariant.  See §4.4 helper signature.
3. **Q3 — `SendOutcome` shape (closed: modified A).**  Drop W7-minimal `SendOutcome`.  Do NOT replace with bare `RoutingDecision` — the OK arm carries `CheckAck.id` (server_fiscal_no) which RoutingDecision can't express without losing the typed routing structure.  Wrap in `WireDecision::{Sent { server_fiscal_no }, Routed(RoutingDecision)}`.  See §3.
4. **Q4 — Audit event_type (closed: A).**  Closed enum `AuditEvent` with `as_str() -> &'static str`.  All event_types written into `audit_log.event_type` go through this enum; new events require enum extension AND a fixture asserting it.  Mirrors W7.1 `OutcomeKind` precedent.  See §3.
5. **Q5 — Pattern B retry-path proof (closed: B + W7-style spy).**  Two layers:
   - **transport_trace row count.**  Assert exactly 1 trace row per wire attempt for the test doc; duplicate rows from `(ErrorRetryable, Sent)` re-entry would fail.
   - **W7-style fresh-read spy inside `send_chk`.**  Source state for the re-attempt MUST be `ErrorRetryable → Sending`; the spy callback opens a fresh tokio runtime + cloned pool + reads `fiscal_documents.state` and asserts `'SENDING'` (NOT `'SENT'`) at wire-call time.  Mirrors the W7.5 Pattern B ordering proof shape.

---

## 8. Apply order (post-GO)

1. **W10.1 first** — `error_routing.rs` pure-fn + 21 unit tests inside the file.  Pure Rust; no DB; trivial to verify.  Day budget: 1.5d.
2. **W10.2** — wire into `stage_send.rs`, replace W7 minimal classify.  Update existing W7.5 fixtures that asserted W7-conservative `Retryable::Server` shape.  Day budget: 0.5d.
3. **W10.3** — `set_mode_blocked_tx` helper + integration in 4-b closure.  Day budget: 0.25d.
4. **W10.4** — MAC recovery -12 path: hash extractor + re-sign helper extraction + recovery orchestrator + integration into `stage_send::run`.  Day budget: 1.0d.
5. **W10.5** — 21 routing fixtures + MAC recovery fixture + Pattern B retry-path fixture.  Day budget: 1.0d.

Estimated total: **4 days** (matches plan Task 9 budget).

---

## 9. Verify hooks

- `cargo test -p prro --test write_path_dps_error_routing` — 21 fixtures.
- `cargo test -p prro --test write_path_mac_recovery` — 3 fixtures (happy + hash-not-extractable + repeat-on-12).
- `cargo test -p prro --lib error_routing` — 21 unit tests + 4 sanity checks.
- `cargo test -p prro --test with_immediate_no_foreign_io` — W3 scanner stays 8/8 (must not regress when MAC recovery sign helper is invoked).
- `cargo test -p prro` — full suite stays 295+ passed (W10 adds ~25 fixtures).
- `cargo fmt -p prro --check` clean.
- `cargo clippy -p prro --tests --no-deps` — 0 prro warnings.

---

## 10. Carry-forward from earlier slices (active)

- W7 freeze §6: «Full DpsError → DocState dispatch table» — **closed by this slice**.
- W7 freeze §6: «KVT1 inline production path» — still deferred (W11 or post-M3a).
- W8 freeze §6: «`Sent → Kvt1 → Kvt2` quittance pipeline» — still deferred.
- W8 freeze §11 F1-bis (`mark_rejected_tx → Result<bool>`): bd `9qd.1.1`.
- W8 freeze §12 F1-bis (`read_state_tx` thinner helper): bd `9qd.1.1`.
- W8 freeze §13 (this PR's senior-review tracking pattern): apply to W10 if review surfaces findings.

---

## 11. Risks (quick scan)

- **R-W10-1 (low, mitigated by exhaustive match):** DpsError is **NOT** `#[non_exhaustive]` (verified at `transports/dps/error.rs:15`).  W10 leverages this: `route_dps_error` uses an exhaustive match with no `_` catch-all — adding a 9th DpsError variant in the future BREAKS the build, exactly the safety net we want.  The reviewer flagged this in B1: enum-level safety is structural; only the raw `i32` Server-code dispatch needs a fail-closed `_` arm (since `i32` isn't an enum).
- **R-W10-2 (low):** MAC recovery re-sign uses W6 helpers; if W6 internals change shape, MAC recovery breaks.  Mitigation: extracted helper `re_sign_after_mac_recovery` — explicit API boundary; W6 changes must update the helper.
- **R-W10-3 (low):** `Server{-12}` regex extraction depends on DPS server message format `store {64hex}`.  If DPS changes the format, MAC recovery falls back to `HashNotExtractable` → terminal Rejected.  Acceptable; not a regression vs current state where W7 routes -12 to generic Retryable.  Forensic audit captures the original message for operator review.
- **R-W10-4 (medium):** `is_live_send` parameter — if a future caller passes the wrong bool, routing decision targets the wrong source state.  Mitigation: caller convention enforced at the call sites — stage_send.rs always passes `true`; W9 reconciliation worker always passes `false`.  Documented on the fn doc.

---

## 12. Sign-off checklist (v2 — conditional GO accepted)

- [x] **All 5 open questions in §7 finalised** (Q1=A, Q2=A scoped, Q3=modified A, Q4=A, Q5=B+spy).
- [x] **§8 apply order accepted** (W10.1 → W10.2 → W10.3 → W10.4 → W10.5).
- [x] **§6 out-of-scope items frozen** (last_chk probe execution → W9; Sent→Kvt1→Kvt2 → separate slice; offline-pool → M3b).
- [x] **`is_live_send` contract documented** on `route_dps_error` + `route_send_result` doc; caller convention enforced at sites (stage_send=true; W9=false).
- [x] **MAC recovery counter strategy = Q1.A** — migration 012 dedicated column.
- [x] **B1 close** — exhaustive match on `DpsError` (no `_` catch-all); only `Server{code:i32}` has fail-closed default arm.
- [x] **B2 close** — `target_state` always `DocState`; NotFound/QueryNotSupported on live → ErrorRetryable+WrapperBug+CRITICAL.
- [x] **B3 close** — MAC recovery atomic multi-write of (previous_hash, unsigned_xml_sha256, PAYLOAD_XML, SIGNED_XML, mac_recovery_attempts) inside one `with_immediate`.
- [x] **B4 close** — `mac_recovery_repin_tx` distinct from W6 pin (no `node_state` read; uses DPS-extracted hash; source state SENDING; counter guard).

GO confirmed for W10.1 → W10.5 apply order.
