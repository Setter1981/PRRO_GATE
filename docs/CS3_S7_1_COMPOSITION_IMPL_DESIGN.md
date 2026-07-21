# CS-3 S7-1 — Composition Impl-Design (narrow open seams)

**Status:** executable build spec for the atomic cutover. **NOT a re-design** — the design-of-record
`CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` (FROZEN §11) and the seam map
`CS3_S7_1_CUTOVER_BUILD_SEQUENCE.md` are the oracle. This doc resolves ONLY the narrow
implementation seams those docs delegate to implementation. Every anchor verified by direct read
against worktree `cs3-de-slice2` / branch `cs3-de-slice7-cutover` (off `main`/`1999ff1`), 2026-07-21.

Scope: Q1 binding source · Q2 record-tx trace+audit · Q3 apply-orchestration home ·
Q4 run() shape + sign_ctx · Q5 NewReservation + envelope_hash.

---

## Q1 — DPS protocol binding source (linchpin)

**DECISION: (b) — a fixed, config-independent immutable binding VALUE, constructed once at the
authorize-tx call site inside `stage_send.rs::run_one_attempt` and reused verbatim as the
`port_binding` argument to `submit_authorized`. Do NOT add a `binding()` method to `DpsChannel`.**

The sanctioned constant (design §2 "an exactly equivalent immutable adapter value"):

```rust
// stage_send.rs — module-private const fn, the single production source of the binding tuple.
fn production_dps_binding() -> prro_domain::delivery::DpsProtocolBinding {
    use prro_domain::delivery::{DpsProtocolBinding, DpsProtocolId, ProtocolContractVersion};
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}
```

**Grounded rationale.**
- The `DpsChannel` trait (`transports/dps/channel.rs:22-138`) has NO `binding()` method today; adding
  one touches the production `GrpcDpsChannel` (`grpc.rs:233 impl DpsChannel`) PLUS every test stub —
  `inline.rs:1966 StubLastChk`, `last_chk_probe.rs:148 StubChannel`, the counting
  `StubDpsChannel` (`tests/common/mod.rs:88`), and the impls under `tests/` (`write_path_stage4_send.rs`,
  `b10_offline_session_handshake.rs`, `return_online_probe.rs`, `write_path_deterministic_replay.rs`,
  `backup_restore.rs`, `api_surface_no_db_handle.rs`, `live_dps_extended_smoke.rs`). That is a wide,
  mechanical blast radius for a value that is **provably constant**.
- The binding values are fixed by construction at `53c5b13`/`1999ff1`: every `DpsProtocolBinding`
  literal in the tree is `{FscoZzd, ProtocolContractVersion(1), None, None}` — the domain default pin
  (`evidence.rs:538-542`), the grpc drift-pin helper (`grpc.rs:680-686`), and all classify/record pins.
  `cpv`/`ecr` are provably always `None` (round-2 #2 notes both always None at `53c5b13`,
  `grpc.rs:683-684`; there is no config column and no runtime producer). `protocol_id`/`contract` are
  fixed `FSCO_ZZD`/`1`.
- The fiscal config has NO binding columns: the binding columns
  (`dps_protocol_id/protocol_contract_version/capability_profile_version/endpoint_config_revision`) exist
  ONLY on `delivery_reservation` (migration `032:81-84`), not on `fiscal_number_config`. There is no
  config-derived source to read; a `binding()` accessor would return the same hardcoded constant.
- **AO-2 stays meaningful.** The echo-check in `submit_authorized:59-73` compares the token's captured
  binding against `port_binding`. Both derive from the SAME const, so the happy path always echoes. AO-2
  is a **rebind/tamper guard on the seam between authorize and wire**, not a config-negotiation: the
  compile-fail teeth (S7-P2-2) and the `wrong_binding` test (`submit_authorized.rs:220`) prove a
  mismatch produces ZERO wire calls. Load-bearing property preserved: the token carries the binding
  captured at authorize-time (`authorize_submission:507-510` → `Authorization` ctor `:573-583`), and the
  wire seam refuses if the live port value ever diverges.

**Exact construction site.** In `run_one_attempt` (`stage_send.rs:1238`), after the authorize-tx returns
the `Authorization` and before the wire call: `let port_binding = production_dps_binding();` then
`submit_authorized(dps_channel, &port_binding, auth, envelope, doc_type).await`. The SAME const is also
what the authorize-tx uses to populate `NewReservation`'s 4 binding fields (Q5) — one source, so the
token and the port value are identical by construction.

**IMPL-OPEN (backlog, non-blocking):** when CS-6 introduces `EvpzDps` / a real capability profile, this
const graduates to a per-FN config read (shift fixes `protocol_id` at shift-open per `mod.rs:66-68`
PB-4a). Not in S7-1 scope — `EvpzDps` has no live wire path today.

---

## Q2 — record-tx trace-completion + audit derivation (gap #3)

**DECISION: (a) — PRESERVE the legacy `WireDecision` and drive trace-complete + outcome audit from it,
BYTE-equivalent to legacy. `submit_authorized` must SURFACE its `_legacy` arm instead of discarding it.
The trace-complete + audit land in the RECORD-orchestration wrapper in `stage_send.rs` (NOT inside the
repo `record_outcome`, and NOT inside `submit_authorized`).**

This splits into three concrete edits.

### Q2.1 — `submit_authorized` returns the legacy `WireDecision` alongside `AttemptObservation`

Today `submit_authorized` computes `let (_legacy, observation) = channel.send_chk_observed(...)`
(`submit.rs:81`) and DISCARDS `_legacy`. The legacy `route_send_result(wire_result, doc_type, true)`
(`stage_send.rs:1587`) is the ONLY thing that produces the `WireDecision` that `build_attempt_completion`
+ the STAGE_SEND_RESULT/routed audit consume verbatim. Deriving completion/audit from
`classify`/evidence instead (option b) would re-implement the byte-exact mapping and is **rejected** —
the ApplyPlan pin (§4.1) requires unchanged incumbent rows to be exactly equal, and the audit dim is
in the 7-tuple; re-deriving risks a fourth-delta divergence.

Change the return type so the caller can compute the WireDecision:

```rust
// submit.rs — surface the legacy Result so the record wrapper can route it exactly as 4-b did.
pub async fn submit_authorized(
    channel: &dyn DpsChannel,
    port_binding: &DpsProtocolBinding,
    auth: Authorization,
    envelope: CheckEnvelope,
    doc_type: DocType,
) -> Result<(AttemptObservation, Result<CheckAck, DpsError>), SubmitRefused> {
    // ... rebind + AO-2 unchanged (:52-73) ...
    let (legacy, observation) = channel.send_chk_observed(envelope).await;   // was `_legacy`
    let response = map_send_reply(observation.evidence(), doc_type);          // :85 unchanged
    let evidence = SubmissionEvidence::Started { response, binding: port_binding.clone(), envelope_hash };
    Ok((AttemptObservation::from_authorization(auth, evidence), legacy))
}
```

`CheckAck`/`DpsError` are already the channel's types (`channel.rs:12-15`). The caller then runs the
IDENTICAL legacy sequence: `route_send_result(legacy, doc_type, true)` → `WireDecision`. Note the
`EmptyServerFiscalNo` guard (`stage_send.rs:1594-1598`) becomes the `OkButNoFiscalNumber` ApplyPlan row
(design §4 "the `EmptyServerFiscalNo` condition becomes the typed `OkButNoFiscalNumber` ApplyPlan row")
— it must NOT `return Err` and leave an unrecorded `CALL_STARTED`; it flows into the record as a normal
HOLD leaf.

### Q2.2 — record args from evidence (`classify` + `EvidenceDiscriminant` + `ObservedOutcomeV1`)

`record_outcome` (`delivery_reservation.rs:634`) takes `(&AttemptObservation, &ObservedOutcomeV1,
&EvidenceDiscriminant)`. The caller builds those from `obs.evidence()` (the `Started` evidence),
mirroring the `tests/record_outcome.rs` helper pattern (`:445-465`):

```rust
let classified = prro_domain::delivery::classify(obs.evidence());                 // mod.rs:908
let disc       = prro_domain::delivery::EvidenceDiscriminant::from_evidence(obs.evidence()); // evidence.rs:211
let generation = AuthorizedGeneration::started(obs.authorized_generation());       // Started witness
let outcome    = ObservedOutcomeV1::record(&classified, remote_correlation_id, generation)?; // mod.rs:1160
```

`remote_correlation_id` is the record-step-only `Option<BoundedText>` (design: known at record, not to
the pure classifier — `mod.rs:1155`); for the live send path it is `None` unless the evidence carries a
correlation id (match today's behaviour: legacy 4-b writes no correlation id).

### Q2.3 — the RECORD orchestration wrapper owns trace-complete + audit

`record_outcome` deliberately does the reservation CAS + axes + early STOP/BLOCKED but NOT the
`transport_trace` completion nor the outcome audit (confirmed `:634-723`; GAP per build-sequence line
28). The legacy 4-b did both inside its `with_immediate` (`build_attempt_completion` `:1892-1893`,
`complete_tx` `:1895`, STAGE_SEND_RESULT/routed audit `:1919-1966`). Put them in a **record wrapper in
`stage_send.rs`** so the SAME `BEGIN IMMEDIATE` holds record + trace-complete + audit atomically (design
§4 record-transaction: "persists evidence/axes/effect and PENDING_APPLY, completes the existing
transport_trace, appends the outcome audit, applies the early STOP/BLOCKED"):

```rust
// stage_send.rs — the record transaction (one BEGIN IMMEDIATE). Signature owns the exact 4-b bytes.
async fn record_transaction(
    pool: &SqlitePool,
    obs: &AttemptObservation,
    outcome: &ObservedOutcomeV1,
    disc: &EvidenceDiscriminant,
    decision: &WireDecision,                 // from route_send_result(legacy,…) — Q2.1
    forensics: Option<&(Option<i32>, &'static str, String)>, // extract_wire_forensics, :1583-1586
    started: String, finished: String,       // wire_call_started/finished_at, :1562/:1574
    doc: DocumentId, attempt_no: i32,
) -> Result<(), StageSendError> {
    with_immediate(pool, move |tx| Box::pin(async move {
        // 1. authority CAS + axes + early STOP/BLOCKED (repo — unchanged body).
        delivery_reservation::record_outcome(tx, obs, outcome, disc).await.map_err(bridge_record)?;
        // 2. complete the existing transport_trace — VERBATIM 4-b :1892-1901.
        let completion = build_attempt_completion(decision, forensics, started, finished);
        let outcome_kind_str = completion.outcome_kind.as_str();
        let rows = transport_trace::complete_tx(tx, doc, attempt_no, completion).await?;
        if rows == 0 { return Err(StageSendError::TraceMissingAtComplete { document_id: doc, attempt_no }.into()); }
        // 3. outcome audit — VERBATIM 4-b :1903-1966 (STAGE_SEND_RESULT/routed event, same payload).
        append_stage_send_result_audit(tx, doc, decision, attempt_no, outcome_kind_str).await?;
        Ok::<_, anyhow::Error>(())
    })).await.map_err(bridge_anyhow)
}
```

`build_attempt_completion` (`stage_send.rs:914`) and the audit block (`:1919-1966`) MOVE verbatim from
the deleted 4-b into this wrapper / the `append_stage_send_result_audit` helper — the byte-identity the
ApplyPlan pin requires is achieved by LITERAL RELOCATION, not re-derivation. The `outcome_kind` mapping
(`wire_decision_to_outcome_kind`, `OutcomeKind` `transport_trace.rs:41-76`) is untouched. Early
STOP/BLOCKED stays inside `record_outcome` (repo, `:698-721`) — it fires in the same tx, so ordering vs
trace/audit is irrelevant to correctness.

**Rationale for the wire/decision split:** the doc/state target CAS (`Sending→Sent/Rejected/ER`), SFN,
seed and shift edges MOVE to the APPLY tx (Q3 / `apply_outcome`), NOT the record tx. Record owns
evidence + trace + audit + safety-halt; apply owns the fiscal projection. This is the design's two-commit
boundary (§4). The trace/audit are evidence-of-the-wire and belong with record.

---

## Q3 — apply-orchestration home + confirm_shift_edge + boot rewire

**DECISION: NEW module `services/write_path/apply_orchestration.rs` owning a single shared fn
`apply_recorded_outcome(pool, reservation_id) -> Result<ApplyResult, ApplyOrchestrationError>`, called
by BOTH the live cutover `run_one_attempt` and `reservation_boot_pass::apply_one`. It derives
closing-cash OUTSIDE the tx, then one `BEGIN IMMEDIATE`: `apply_outcome(tx, res_id)` (repo) + online
shift edges 3/10 + closing-cash. `confirm_shift_edge` is MADE `pub(crate)` and CALLED (not moved).**

### Q3.1 — module + signature

```rust
// services/write_path/apply_orchestration.rs (NEW)
pub async fn apply_recorded_outcome(
    pool: &SqlitePool,
    reservation_id: ReservationId,
) -> Result<ApplyResult, ApplyError> {
    // (0) read the durable doc/shift context this apply needs for the shift edge, OUTSIDE the tx.
    //     From delivery_reservation (fiscal_number, document_id) + fiscal_documents
    //     (doc_type, shift_id, offline_fiscal_no online-origin discriminator).
    let ctx = load_apply_context(pool, reservation_id).await?; // None if not PENDING_APPLY → caller no-op
    // (1) closing-cash derive OUTSIDE the write-tx (invariant #1) — RELOCATED VERBATIM from
    //     stage_send.rs:1629-1700 (only the CLOSE edge, online-origin). Derived from durable
    //     document/shift/ledger state immediately before the apply tx (design §4: NOT an ephemeral
    //     value surviving the record→apply crash window).
    let closing_cash_kop = derive_closing_cash_for_apply(pool, &ctx).await; // :1636-1693 body

    // (2) one BEGIN IMMEDIATE: repo apply + shift-confirm + cash.
    with_immediate(pool, move |tx| Box::pin(async move {
        let res = delivery_reservation::apply_outcome(tx, reservation_id).await?; // repo, :795
        // Fire shift edges ONLY when apply actually released an Accepted online-origin doc.
        if res.applied && ctx.online && res.server_fiscal_no.is_some() {
            match ctx.doc_type {
                DocType::ShiftOpen => confirm_shift_edge(tx, &ctx.fiscal_number, ctx.shift_id, ctx.doc,
                    ShiftState::Opening, ShiftState::Opened, "edge3_open", None).await?,
                DocType::ZReport | DocType::ShiftClose => confirm_shift_edge(tx, &ctx.fiscal_number,
                    ctx.shift_id, ctx.doc, ShiftState::Closing, ShiftState::Closed, "edge10_close",
                    closing_cash_kop).await?,
                _ => {}
            }
        }
        Ok::<_, anyhow::Error>(res)
    })).await
}
```

**Grounded rationale.**
- `apply_outcome` (repo) fires ZERO shift edges + no closing cash (build-sequence line 32; ApplyPlan
  reconciliation gap 1). Live 4-b fired `confirm_shift_edge` for online ShiftOpen (edge3
  `stage_send.rs:1831-1843`) / ZReport|ShiftClose (edge10 `:1844-1856`) + `shifts.cash_balance_kop`.
  Without this, online shifts silently never advance and cash carry is lost.
- **Repo must not call up into `services::*`** (design §4): `apply_outcome` stays a pure repo fn; the
  shift-confirm (which lives in `stage_send.rs`, a service) is invoked by the ORCHESTRATION service, in
  the same tx, AFTER the repo apply returns. `confirm_shift_edge` (`stage_send.rs:1134`) is currently
  private `async fn`; change to `pub(crate)` and call it from the new module. Do NOT move it — moving
  churns `stage_send.rs` (it also references `emit_shift_confirm_audit`, `ShiftState`, the transition
  service); calling it in place is the minimal diff. The design sanctions call-or-move; call is narrower.
- **Shift-edge gating.** 4-b fired the edge inside the `WireDecision::Sent` arm (`:1750`) gated on
  `offline_fiscal_no.is_none()` (`:1829`). The apply-side equivalent is `res.applied && ctx.online &&
  res.server_fiscal_no.is_some()` — `apply_outcome` only stamps `server_fiscal_no` on the online/offline
  `Accepted` release (`:883-900`), so `Some(sfn)` is the exact Accepted discriminator; `ctx.online`
  (`offline_fiscal_no IS NULL`, matching `apply_outcome:877`) keeps the drain from double-firing (drain
  owns offline pending-drain edges, `backlog_drain.rs:2460`). This preserves the 4-b gate byte-for-byte.
- **Seed drift gate stays subsumed.** The design (§2.1 / §4) MOVES the online predecessor-equality
  (`last_known_unsigned_xml_sha256 == previous_hash`) to the PRE-WIRE authorize-tx (Q4). `apply_outcome`
  advances the seed UNCONDITIONALLY (`:893` via `node_advance_seed`) under the generation-CAS
  (`:842-861`); the fence + gen-CAS + pre-wire equality together subsume the old in-tx drift `ensure!`
  (`stage_send.rs:1800-1808`, skipped on `-12`). No drift gate is added in apply — that is exactly the
  R3 `-12` divergence fix (a `-12` never reaches apply as Accepted; it is a HOLD).

### Q3.2 — `load_apply_context` reads

`load_apply_context(pool, reservation_id)` does two pool reads (read-only, invariant #1 OK):
`SELECT fiscal_number, document_id, state, apply_state FROM delivery_reservation WHERE reservation_id=?`
then `SELECT doc_type, shift_id, offline_fiscal_no FROM fiscal_documents WHERE document_id=?`. Returns
`{fiscal_number, doc, doc_type, shift_id, online: offline_fiscal_no.is_none()}`. If not
`OUTCOME_OBSERVED+PENDING_APPLY`, return `None` and the caller treats it as a benign no-op (matches
`apply_outcome`'s own `NotPendingApply`/APPLIED idempotency, `:830-840`).

### Q3.3 — boot rewire (§4.1 one projection)

`reservation_boot_pass::apply_one` (`reservation_boot_pass.rs:84-104`) currently calls raw
`delivery_reservation::apply_outcome` inside its OWN `with_immediate` (`:88-94`). REWIRE it to call
`apply_orchestration::apply_recorded_outcome(pool, reservation_id)` — which owns its own tx — so boot
also fires the shift edges (design §4.1 "Boot replay calls this same apply orchestration; it does not
maintain a second projection table"). The `ApplyClass` classification (`:96-103`) is UNCHANGED: it still
downcasts `ApplyError::HeldNotAutoRelease → Held` and `NodeStateMissing → NodeMissing`; the orchestration
propagates those verbatim (the closing-cash derive + shift-confirm only run on `res.applied`, so a HOLD
returns before them). `apply_one` drops its inner `with_immediate` since the orchestration owns the tx.

---

## Q4 — run()/run_one_attempt straight-line shape + sign_ctx

**DECISION for `sign_ctx`: KEEP the parameter, rename to `_sign_ctx` — ZERO caller edits.** All 7
callers pass `Some(sign_ctx)` uniformly and I confirmed the grep: `inline.rs:910`,
`online_convergence.rs:561`, `boot_phase.rs:3138`, `boot_phase.rs:3751`, `boot_phase.rs:4009`,
`backlog_drain.rs:1323`, `backlog_drain.rs:2969` (7 = the design's "7 callers" count). Removing the param
would edit all 7 + churn the signature the static pin depends on; keeping `_sign_ctx` is the minimal
diff. `sign_ctx` becomes unused because R3 collapses the MAC loop (envelope is pre-signed; no MAC
re-sign). S7-3 cleanup may remove it later.

**`run` collapses (R3).** The MAC loop `stage_send.rs:1048-1116` is DELETED. `run` becomes a thin
straight-line pass-through to `run_one_attempt` (no loop, no `mac_recovery_invoked`, no
`run_mac_recovery`, no `Resigned=>continue`):

```rust
pub async fn run(pool, dps_channel, doc, _sign_ctx: Option<&SigningContext>)
    -> Result<StageSendOutcome, StageSendError>
{
    run_one_attempt(pool, dps_channel, doc).await   // straight-line; -12 is a recorded HOLD, not a loop
}
```

**`run_one_attempt` new shape (4 phases).**

```rust
async fn run_one_attempt(pool, dps_channel, doc) -> Result<StageSendOutcome, StageSendError> {
    // ── PHASE 1: AUTHORIZE tx (replaces 4-pre :1244-1551) ──────────────────────────────
    let pre = with_immediate(pool, |tx| Box::pin(async move {
        // 1a. unchanged 4-pre body :1248-1450 EXCEPT:
        //     - R2: drop ErrorRetryable from the allowlist at :1269 AND the :1420 re-derivation
        //       match (keeps the unreachable! honest + both allowlists in sync).
        //       → allowlist becomes {Signed, OfflineLocalAck}.
        //     - the state-allowlist gate :1267-1274, STOP-O3-1 :1292-1303, signer :1324-1334,
        //       signed-artifact read :1339-1343, OLA guards :1364-1390, envelope build :1394-1397,
        //       source→Sending CAS :1428-1441, submission_attempted stamp :1446-1450 — ALL UNCHANGED.
        // 1b. §2.1 ONLINE-ORIGIN predecessor equality — BEFORE authorize_submission's CALL_STARTED:
        if inputs.offline_fiscal_no.is_none() {   // online-origin only
            let ns_seed = node_state seed (last_known_unsigned_xml_sha256);
            anyhow::ensure!(ns_seed == inputs.previous_hash,
                "S7-1 §2.1: online predecessor drift — refuse authorize, ZERO wire");
        }
        // 1c. allocate trace :1456-1467 UNCHANGED (request_envelope_sha256 = compute_envelope_hash, Q5).
        // 1d. STAGE_SEND_INTENT_MARKED audit :1477-1486 UNCHANGED.
        // 1e. authorize_submission — mints the token + CALL_STARTED in THIS tx.
        let auth = delivery_reservation::authorize_submission(
            tx, build_new_reservation(&inputs, &envelope), &now_db_format()).await?; // Q5
        Ok(PreOutcome::Marked { auth, envelope, attempt_no, doc_type, shift_id,
            offline_fiscal_no, previous_hash, unsigned_xml_sha256, fiscal_number })
    })).await.map_err(bridge_anyhow)?;
    let PreOutcome::Marked { auth, envelope, attempt_no, doc_type, .. } = pre else { /* :1536-1550 */ };

    // ── PHASE 2: WIRE (outside any lock) — submit_authorized is the SOLE send_chk_observed ──
    let port_binding = production_dps_binding();                                  // Q1
    let wire_started = now_db_format();
    let (obs, legacy) = match submit_authorized(dps_channel, &port_binding, auth, envelope.clone(), doc_type).await {
        Ok(v) => v,
        Err(SubmitRefused::EnvelopeRebind | SubmitRefused::BindingMismatch) =>
            return Err(StageSendError::…), // zero wire; token dropped
    };
    let wire_finished = now_db_format();

    // ── PHASE 3: RECORD tx (Q2) — classify + record_outcome + trace-complete + audit ──
    let forensics = match &legacy { Ok(_) => None, Err(e) => Some(extract_wire_forensics(e)) };
    let decision  = route_send_result(legacy, doc_type, true);                    // :1587 verbatim
    let (classified, disc, outcome) = build_record_args(&obs, remote_corr_none)?; // Q2.2
    record_transaction(pool, &obs, &outcome, &disc, &decision, forensics.as_ref(),
        wire_started, wire_finished, doc, attempt_no).await?;                      // Q2.3

    // ── PHASE 4: APPLY orchestration (Q3) — shared live+boot ──────────────────────────
    apply_orchestration::apply_recorded_outcome(pool, obs.reservation_id()).await
        .map_err(bridge_apply)?;   // HELD (-12/-6/unknown) = expected; STOP already set in record.

    Ok(stage_send_outcome_from(&decision, attempt_no, &forensics))               // :1974-1994 verbatim
}
```

**EXACT deletion boundary:** the WHOLE legacy 4-b second `with_immediate`, `stage_send.rs:1710-1972`
(END = `:1972 .map_err(bridge_anyhow)?;` per impl-plan §4.2). Its constituent writes are re-homed:
`transition_state Sending→target :1729`, `set_server_fiscal_no_tx :1751`, seed advance `:1809` incl. the
`:1800-1808` gate → all now in `apply_outcome` (repo, already landed); shift edges `:1829-1859` → apply
orchestration (Q3); `-11` block `:1877` → `apply_outcome`'s `NodeBlocked` arm (`:914`) + record's early
BLOCKED (`:698-703`); `complete_tx :1895` + audit `:1957` → record wrapper (Q2.3). Also DELETE the MAC
loop `:1048-1116` and its recovery-override helpers become dead (S7-3). The `closing_cash` derive
`:1629-1700` is RELOCATED (not deleted) into apply orchestration (Q3.1 step 1). The `EmptyServerFiscalNo`
guard `:1594-1598` is DELETED (becomes the `OkButNoFiscalNumber` HOLD leaf, Q2.1).

**What of `PreOutcome::Marked` survives:** the fields threaded past the authorize-tx shrink to what the
wire + record + apply need — `auth` (NEW), `envelope`, `attempt_no`, `doc_type`. `fiscal_number`,
`shift_id`, `offline_fiscal_no`, `previous_hash`, `unsigned_xml_sha256` are no longer threaded to a 4-b
closure (apply re-reads them from durable state via `load_apply_context`, Q3.2) — but `previous_hash` +
`offline_fiscal_no` ARE still needed INSIDE the authorize-tx for the §2.1 equality (1b), and
`unsigned_xml_sha256` for the trace hash is computed in-tx (Q5). `mac_recovery_attempts` is DROPPED from
`Marked` (no MAC path). The post-authorize `match pre` block (`:1504-1551`) keeps its non-`Marked` arms
verbatim.

---

## Q5 — NewReservation construction + envelope_hash

**DECISION: the authorize-tx builds `NewReservation` from the pre-CAS `SendInputs` + the built
`CheckEnvelope`, with the binding from Q1's `production_dps_binding()` and `envelope_hash =
SHA256(envelope.check_sign)`. The trace's `request_envelope_sha256` is a DIFFERENT hash
(`compute_envelope_hash = SHA256(prost(gen::Check))`) and is unchanged.**

**Two DISTINCT hashes — confirmed:**
- **Token/reservation `envelope_hash`** = `SHA256(envelope.check_sign)` (the CMS-signed blob).
  `submit_authorized:53-56` recomputes `Sha256::digest(&envelope.check_sign)` and rebind-checks it
  against `auth.envelope_hash()`. So the authorize-tx MUST store `SHA256(check_sign)` in
  `NewReservation.envelope_hash` or the rebind guard would false-fail. Migration `032:85` comments this
  as "protocol-specific" and length-32.
- **Trace `request_envelope_sha256`** = `compute_envelope_hash(&envelope)` = `SHA256(prost(gen::Check))`
  (the FULL wire proto, `stage_send.rs:1452-1453` / `transport_trace.rs:78-87` — hashes all fields, not
  just `check_sign`). Stays at `stage_send.rs:1456-1462` UNCHANGED (Phase-1 step 1c).

These are computed at two sites in the SAME authorize-tx and stored in two columns; they are NOT
interchangeable. The `NewReservation` literal:

```rust
fn build_new_reservation(inputs: &SendInputs, envelope: &CheckEnvelope) -> NewReservation {
    let b = production_dps_binding();                                    // Q1
    let check_sign_hash: [u8; 32] = Sha256::digest(&envelope.check_sign).into();  // token hash
    NewReservation {
        reservation_id: fresh_reservation_id(),           // 16-byte id (see IMPL-OPEN)
        document_id: inputs.document_id,                  // == `doc`
        fiscal_number: inputs.fiscal_number.clone(),
        dps_protocol_id: b.protocol_id.as_str().to_string(),          // "FSCO_ZZD"
        protocol_contract_version: i64::from(b.contract_version.0),   // 1
        capability_profile_version: b.capability_profile_version.map(|v| i64::from(v.0)), // None
        endpoint_config_revision: b.endpoint_config_revision.map(|v| i64::from(v.0)),     // None
        envelope_hash: check_sign_hash,                   // SHA256(check_sign) — rebind guard
    }
}
```

`NewReservation` fields confirmed at `delivery_reservation.rs:52-65`. The binding string/int conversions
mirror `submit_authorized:61-70`'s echo (so token == port == reservation by construction). `authorize_
submission` captures these into the `Authorization` (`:506-510 → :573-583`), and `submit_authorized`
echoes them (Q1).

**IMPL-OPEN — `reservation_id` generation:** the seam map does not name the id source. Recommendation:
a fresh random 16-byte id per attempt (matches the `[u8;16]` `ReservationId` type `:42` and the
per-attempt-no design); it must be unique per `(document_id, attempt_no)` — the `insert` fn derives
`attempt_no` in-tx (`:86-92`) and the `UNIQUE(document_id, attempt_no)` + `no_replace` trigger are the
backstop. A deterministic `SHA256(document_id || attempt_intent)` is also acceptable but a random UUID
byte-array is simplest and collision-safe. Decide at build time; either satisfies the DDL.

---

## Cross-cutting: static-pin flip (part of the atomic commit)

Wiring the composition makes `stage_send::run` reference `authorize_submission` / `record_outcome` /
`apply_outcome` → the INACTIVE denylist pins `migration_032::p03` + `migration_033::rg08`
(`tests/support/inactive_lifecycle_scan.rs`) go RED. Retarget to the POSITIVE sole-seam (S7-P2-2):
`send_chk_observed` EXACTLY 1 call-site (inside `submit_authorized`), `submit_authorized` EXACTLY 1
caller (`stage_send::run_one_attempt`), the lifecycle fns called only by the sanctioned cutover sites +
`reservation_boot_pass`. The boot-pass positive pin
(`migration_032::boot_pass_references_only_the_sanctioned_read_apply_subset`,
`reservation_boot_pass.rs:33`) must be updated to allow `apply_orchestration::apply_recorded_outcome` as
the new apply entrypoint boot calls (it no longer calls `apply_outcome` directly).

---

**Nothing in this document has been implemented.** It is the executable build spec for the atomic
cutover (Phase-T teeth RED-first → Phase-C composition → delete 4-b → R1-R7 → #1 → static-pin flip →
Phase-G empty-in-flight gate), per `CS3_S7_1_CUTOVER_BUILD_SEQUENCE.md` build order.
