# CS-3 S7-1 — ApplyPlan 7D Reconciliation & rev9→rev10 Adjudication

**Status:** A2 grounding complete. This document is the authoritative reference for the
S7-APPLY-GRAPH pin (design §4.1 / §8). It records the leaf-by-leaf reconciliation of the
**dossier rev9 normative ApplyPlan matrix** against the **live incumbent** (`classify` /
`route_send_result` / `apply_outcome` / legacy 4-b), and the single adjudication it produced.

## Method

Model-decorrelated workflow (`wf_138e2191-ef4`): 5 parallel mappers extracted one source each
VERBATIM with file:line anchors (dossier §2A matrix, `error_routing::route_send_result`,
`prro-domain::delivery::classify`, `apply_outcome` + legacy 4-b, `EvidenceDiscriminant`), then a
synthesizer reconciled them leaf-by-leaf and adversarially hunted a 4th delta. **All load-bearing
anchors re-verified against live code before this record was written.**

## Result — 28 legal evidence leaves

**0 behaviour mismatch.** 24 leaves reconcile EXACT (dossier defines target_state/audit/probe
by-reference to `route_send_result`, verified live). 3 declared cutover deltas confirmed VERBATIM
(incumbent + target pair). `Accepted` carries an online/offline origin split (seed/shift online-only)
that matches the dossier's origin rule.

7D tuple: `(target_state, retry_class, seed/SFN/shift, node_effect, audit, probe, fence)`.

### The 3 declared deltas (locked incumbent → target pairs)

| Leaf | incumbent (LIVE, locked) | target (declared, cutover flips to) | diverges on |
|---|---|---|---|
| **UnknownStatus** (`-4`/unmapped non-zero) | route Decode arm `error_routing.rs:360-370`: `(ErrorRetryable, ProbeRequired, none, None, StageSendDecodeUnknown/Warning, ProbeRequired[DecodeUnknown], HELD)` | classify `routing_for_indeterminate(UnknownStatus)=TransientRetry` `mod.rs:1024`: `(ErrorRetryable, TransientRetry, none, None, StageSendTransientRetry/Warning, no-probe, HELD)` | retry_class, audit, probe |
| **TLS RemoteAuthStatus** | route RemoteStatus arm `error_routing.rs:314-322`: `(ErrorRetryable, TransientRetry, none, None, StageSendTransientRetry/Warning, no-probe, HELD)` | classify RemoteStatus arm `mod.rs:947-952` projected: `(ErrorRetryable, ProbeRequired, none, ProbeRequired, StageSendProbeRequired/Warning, ProbeRequired[AuthenticatedPeerReply], HELD)` | retry_class, node_effect, audit, probe |
| **OkButNoFiscalNumber** (OK+empty id) | empty-id `EmptyServerFiscalNo` GuardAbort sentinel `stage_send.rs:1596` (doc stays Sending, no ApplyPlan) | `(ErrorRetryable, ProbeRequired, none, ProbeRequired, StageSendProbeRequired/Warning, ProbeRequired[OkButNoFiscalNumber], HELD)` | whole tuple (guard→held) |

> The target-side `ProbeReason::AuthenticatedPeerReply` / `::OkButNoFiscalNumber` do NOT yet exist
> (`ProbeReason` has exactly 3 variants: DecodeUnknown/Code2CloseShift/Code15CloseShift,
> `error_routing.rs:243`). For the FOUNDATION pin the target side is a LOCKED LITERAL (documented
> cutover contract), not driven — the pin asserts the LIVE code produces the **incumbent**, and
> records the target. The 2 variants land with the cutover, not the pin.

## Adjudication rev9 → rev10 — audit-lock over-lock CORRECTION

**Finding (workflow, verified live):** the dossier rev9 pin-rule "target ProbeRequired audit is
LOCKED to `StageSendProbeRequired/Warning` (`error_routing.rs:449`)" is **over-broad**. It was
derived from ONE ProbeRequired leaf (CloseAmbiguous) and generalised to the whole `ProbeRequired`
routing class. But `ProbeRequired` is a routing/node-effect class, **not a functional dependency
for the audit event**: `MissingStatus` (proto `status==0`) is also `ProbeRequired` yet its LIVE
incumbent through `route_send_result` is the `DpsError::Decode` arm →
`StageSendDecodeUnknown/Warning` (`error_routing.rs:362-370`), NOT `StageSendProbeRequired`.

**rev10 normative (architect-adjudicated 2026-07-20):** SCOPE the audit-lock. This is an over-lock
CORRECTION of the oracle, **NOT a 4th behaviour delta** (the live behaviour is unchanged):

- **MissingStatus** → exact incumbent `StageSendDecodeUnknown/Warning`.
- **CloseAmbiguous** (`-2/-15` on close/Z) → `StageSendProbeRequired/Warning` (already live).
- **RemoteAuthStatus** + **OkButNoFiscalNumber** → declared **target**-deltas → `StageSendProbeRequired/Warning` (cutover flips).
- every other row → exact incumbent, full 7D pair.

**Required tooth (architect):** the graph pin MUST RED if `MissingStatus`'s audit is swapped to
`StageSendProbeRequired` — i.e. the pin structurally distinguishes Decode-sourced ProbeRequired from
CloseAmbiguous/target-delta ProbeRequired. A revert-canary must prove it.

## Pin design (S7-APPLY-GRAPH)

REQUIRED-gated Rust integration test. **(1) Enumerate** every leaf: iterate the closed
`EvidenceDiscriminant` set (11 kind tags; assert count==11 via the existing
`roundtrip_all_eleven_leaves` oracle `evidence.rs:623` so a NEW variant breaks the pin), fan
`NoResponse` over 5 causes, `Rejected` over 13 verdicts, `UnknownStatus` over representative codes
outside `UNKNOWN_STATUS_FORBIDDEN_CODES` (`evidence.rs:115`), cross the origin axis for `Accepted`
and `Rejected`. **(2) Compute the incumbent 7-tuple** by driving REAL live surfaces:
`classify(&SubmissionEvidence)` for {certainty,provenance,routing,node_effect};
`route_send_result`/`route_dps_error(&DpsError, doc_type, true)` for
{target_state,retry_class,audit,probe} (map each leaf to the DpsError/WireDecision shape the live
send path produces); seed/SFN/shift/fence by driving `apply_outcome` against a seeded in-memory
reservation and reading back `fiscal_documents.state`/`server_fiscal_no` + `node_state` seed +
`shifts.cash_balance_kop`, or asserting `ApplyError::HeldNotAutoRelease` for HOLD leaves.
**(3) Assert** a data-driven `leaf -> expected 7-tuple` table: ~24 unchanged leaves EXACT (equality
structural, pulled live); the 3 declared leaves as the EXACT (incumbent, target) pair.
**(4) Teeth:** any leaf with `incumbent != normative` AND not in the 3-delta allowlist → FAIL (the
"4th delta = design failure" gate); plus the MissingStatus audit revert-canary. Prove empirically.

**HARD RULE (P4 losslessness):** read leaf identity / fiscal_number / verdict / raw-code / cause
from the `EvidenceDiscriminant` `evidence_*` columns (channel A). NEVER reconstruct from
`ObservedOutcomeV1` (channel B is lossy — collapses the 5 NoResponse causes, the 8 TerminalReject
verdicts, hides `Accepted`'s fiscal_number and `UnknownStatus`'s code, and cannot recover
CloseAmbiguous `-2` vs `-15`).

### Pin structure — implementation finding (faithful decode, crate-internal)

The routing-dim leaves must be constructed via the **REAL decode path**, NOT hand-built `DpsError`
values. The leaf → `DpsError` mapping is non-obvious and MUST NOT be guessed: e.g. `route_server_code`
(`error_routing.rs`) has explicit arms for `-2/-3/-5/-6/-7..-10/-11/-12/-15/-16` but **not `-4`** (an
unknown code there routes to `WrapperBug`), yet the reconciliation's `UnknownStatus` incumbent is the
`Decode`→`ProbeRequired` arm — which only holds if `observe_check_reply` maps `status==-4` to
`DpsError::Decode`, not `Server{-4}`. Guessing the shape asserts an incumbent for a path the live
code never reaches (the workflow flagged exactly this). Therefore:

- Construct each wire leaf via `observe_check_reply(chk(status, id))` (status leaves) and
  `observe_tonic_status(status, peer_auth)` (transport / TLS leaves) — the SAME decode the live send
  path uses — then drive `route_send_result(legacy, doc_type, true)` + `classify(&started(map_send_reply(&raw, dt)))`.
  This resolves the `-4` / `MissingStatus` / empty-id shape ambiguity BY CONSTRUCTION.
- `observe_check_reply` / `observe_tonic_status` are `pub(in crate::transports::dps)`, so the pin
  must live **crate-internal in `transports::dps`** (extend the existing `grpc.rs` §4.6 `drift_check`
  pattern into a full normative `RoutingDecision` table), NOT as an integration test with
  hand-built `DpsError`. It is still REQUIRED-gated (same nextest). Design's "integration test"
  wording yields to this visibility constraint; note the deviation in the pin's doc-comment.
- The pin asserts each leaf's live `RoutingDecision` (target_state, retry_class, audit_event,
  node_mode_flip, probe) against the locked rev10 table; the 3 declared deltas as the (incumbent,
  target-literal) pair; the `apply_outcome` seed/SFN/shift/fence dims as a SEPARATE follow-on
  (DB-driven per outcome class) so the first pin commit stays pure + tractable.

## apply_outcome coverage — cutover gaps (NOT pin blockers; flagged for B cutover)

`apply_outcome` (`delivery_reservation.rs:795-965`) is the apply projection but does NOT yet reach
full 4-b parity. These are **cutover** deliverables, not foundation-pin blockers (the pin asserts
apply dims against whichever surface owns them; for now it asserts the incumbent 4-b behaviour and
`HeldNotAutoRelease` for holds):

1. **Shift edges (highest risk)** — `apply_outcome` fires ZERO shift edges / persists no closing
   cash; live 4-b fires `confirm_shift_edge` (Opening→Opened edge3 / Closing→Closed edge10 +
   `shifts.cash_balance_kop`) for online ShiftOpen/ZReport/ShiftClose (`stage_send.rs:1829-1859`).
   Cutover MUST add a shift-confirm hook or online shifts silently never advance + cash carry is lost.
2. **Seed-drift gate** — 4-b guards the online seed advance with
   `ensure!(last_known_unsigned_xml_sha256 == previous_hash)` (skipped when
   `mac_recovery_attempts>=1`); `apply_outcome::node_advance_seed` advances UNCONDITIONALLY. Confirm
   the generation/fence CAS fully subsumes the drift guard, else a chain fork advances silently.
3. **Audit/trace** — `apply_outcome` writes NO `fiscal_document` audit row + touches NO
   `transport_trace`; 4-b writes a per-outcome `audit_log` row + completes the trace
   (`stage_send.rs:1892-1966`). The audit dim regresses unless wired at the cutover call site.
4. **FnConfigError arm — CLOSED** (`c8e05a0`): `apply_outcome` already does
   `doc_from_sending(ErrorRetryable)` for `-13/-14`, matching §4.1.
5. **-12/-6, offline-origin reject** = HOLD by construction (MAC recovery runs in-run in 4-b;
   apply_outcome treats the POST-recovery leaf as HOLD). The pin must cross the origin axis or miss
   the offline-origin reject row.

## Open questions — resolved

- **ProbeReason extension:** target-as-literal for the foundation pin (above); the 2 variants land at
  cutover. Not a pin blocker.
- **Origin axis:** the pin crosses online/offline for `Accepted` and `Rejected` (drain owns offline
  shift/seed; the row count reflects both origins).
- **Cutover ownership of shift-confirm + audit/trace:** deferred to B cutover (gap 1/3 above); the
  foundation pin asserts the incumbent, the cutover wires the owner and flips the assertions.
