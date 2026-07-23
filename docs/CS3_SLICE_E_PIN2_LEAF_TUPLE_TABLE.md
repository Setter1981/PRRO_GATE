# CS-3 Slice E — Pin 2 leaf→tuple table (§3.1 deliverable)

**Status:** implementation design, grounded against `cs3-slice-e` @ `adc7f7c` (off `224ad46`, post Pin 1).
Every anchor re-verified live. **Implemented + verified** (pin GREEN, both revert-canaries bite,
fx06–fx09 behaviour-neutral).

Pin 2 = **Track A total projection**: replace `project_decision_from_evidence(legacy, &classified)`
with a TOTAL `wire_decision_from(disc, &classified, wrapper_overlay)` that derives the wire
`WireDecision` from the sealed evidence leaf + the classifier + the demoted `route_dps_error` overlay —
never from the collapsed legacy `DpsError` as authority. Single prod call site: `stage_send.rs` PHASE-3.

## Signature (grounded correction of the rev4 plan)

The rev4 plan wrote `wire_decision_from(disc, &classified, &diag: &WireDiagnostics)`. Grounding showed
`&WireDiagnostics` is **wrong for the one job it was meant to do** (the WrapperBug overlay): a wrapper-side
bug mints an EMPTY `WireDiagnostics::default()` (the absence path — `observe_from_legacy`), so the sidecar
cannot witness it. The only witness is the legacy `route_dps_error` result. So the third parameter is
`wrapper_overlay: Option<&RoutingDecision>` (the demoted `route_dps_error(&legacy_err, doc_type, true)`),
consulted EXCLUSIVELY on the `NoResponse` leaf. `AttemptObservation` never carried `WireDiagnostics`
anyway (it is dropped inside `submit_authorized`), so no sole-wire plumbing was widened.

## Boundary (rev4 §3.1)

- **Routing authority — from `classified` + `disc` ONLY:** `target_state`, `retry_class`,
  `node_mode_flip`, `probe_hint.reason`, and the happy `Sent{server_fiscal_no}` (SFN from `disc::Accepted`,
  `evidence.rs`).
- **Diagnostic overlay — `wrapper_overlay` (demoted `route_dps_error`):** consulted ONLY on the
  `NoResponse` leaf to preserve a wrapper-side bug's CRITICAL `WrapperBug` decision verbatim. The
  message / DPS status-code still reach the trace via the SEPARATE, untouched `wire_forensics`
  (`extract_wire_forensics`) path — NOT via `wire_decision_from`.

## Grounding notes

1. **`mac_recovery_hint` on the projected decision is DEAD** → `None` on every leaf. Field reads exist
   only in `error_routing.rs` unit tests; the W10.4 MAC orchestrator (`mac_recovery.rs`) reads its own
   `hint` param sourced from the durable `-12` evidence, not this projection. Emitting `None` is
   observably identical (the field never reaches audit/trace/return).
2. **Wrapper-bugs ARE reachable — via the observed evidence collapse.** `Internal` / `NotFound` /
   `QueryNotSupported` / `ServerFiscalIdMismatch` collapse to the ONE `NoResponse{CallFailed…}` leaf
   (`observe_faithful_from_legacy` → `observe_from_legacy`); `classify(NoResponse)` is always
   `TransientRetry`. The S7-1 contract deliberately keeps them SENDING-held with a CRITICAL `WrapperBug`
   audit for operator visibility (`write_path_dps_error_routing.rs` fx06–fx09). Since the classifier lost
   the shape, that decision rides the `wrapper_overlay` — preserved VERBATIM (incl. the distinct
   `StageSendFiscalIdMismatch` audit for the id-mismatch). A `Transport` overlay is non-WrapperBug and
   does NOT apply → the leaf keeps the classifier's `TransientRetry` (the reachable case, behaviour-neutral).
3. **`SigningFailed` (classifier `WrapperBug`) is UNREACHABLE post-wire** (`submit_authorized` returns
   only `Started`). Handled for totality; it is where the classifier's own `WrapperBug` class is authored.

## The table (all 11 `EvidenceDiscriminantKind` leaves — exhaustive match, no `_`)

`R?` = reachable at the post-wire projection site. `cls` = `classified.routing()`.
`flip` = `node_mode_flip` (from `classified.node_effect()`: `NodeBlocked → Some(Blocked)`, else `None`).

| # | leaf | R? | cls | target_state | retry_class | audit_event | severity | probe_reason | source |
|---|------|----|-----|--------------|-------------|-------------|----------|--------------|--------|
| 1 | `Accepted(fid)` | Y | None | — (`Sent{fid}`) | — | STAGE_SEND_RESULT¹ | Info¹ | — | disc(fid) |
| 2 | `Rejected(Verify)` | Y | TerminalReject | Rejected | TerminalReject | STAGE_SEND_REJECTED | **Error** | — | disc(verdict)+cls |
| 3 | `Rejected(Type/Xml/XmlDate/XmlChk/XmlZReport/OfflineId/Close)` | Y | TerminalReject | Rejected | TerminalReject | STAGE_SEND_REJECTED | Critical | — | disc(verdict)+cls |
| 4 | `Rejected(Offline168)` | Y | TerminalReject | Rejected | TerminalReject | STAGE_SEND_NODE_BLOCKED | Critical | — | disc+cls (flip=Blocked) |
| 5 | `Rejected(BadHashPrev)` | Y | MacRecovery | ErrorRetryable | MacRecovery | STAGE_SEND_MAC_HASH_MISMATCH | Warning | — | disc(verdict)+cls |
| 6 | `Rejected(NotPrevZReport)` | Y | OperatorEscalation | ErrorRetryable | OperatorEscalation | STAGE_SEND_OPERATOR_ESCALATION | Error | — | disc+cls |
| 7 | `Rejected(NotRegisteredRro/Signer)` | Y | FnConfigError | ErrorRetryable | FnConfigError | STAGE_SEND_FN_NOT_REGISTERED | Error | — | disc+cls |
| 8 | `UnknownStatus(code)` — cls=TransientRetry (today) | Y | TransientRetry | ErrorRetryable | TransientRetry | STAGE_SEND_TRANSIENT_RETRY | Warning | — | cls |
| 8′| `UnknownStatus(code)` — cls=ProbeRequired (**dormant; Pin 3**) | Y | ProbeRequired | ErrorRetryable | ProbeRequired | STAGE_SEND_PROBE_REQUIRED | Warning | **SubmittedUnknown** | cls |
| 9 | `SaveError` (`-3`) | Y | TransientRetry | ErrorRetryable | TransientRetry | STAGE_SEND_TRANSIENT_RETRY | Warning | — | cls |
| 10 | `CloseAmbiguous` (`-2`/`-15` close-shift) | Y | ProbeRequired | ErrorRetryable | ProbeRequired | STAGE_SEND_PROBE_REQUIRED | Warning | **CloseShiftProbe** | disc+cls |
| 11 | `MissingStatus` (status=0 decode) | Y | ProbeRequired | ErrorRetryable | ProbeRequired | STAGE_SEND_DECODE_UNKNOWN | Warning | **DecodeUnknown** | disc+cls |
| 12 | `OkButNoFiscalNumber` (OK empty id) | Y | ProbeRequired | ErrorRetryable | ProbeRequired | STAGE_SEND_PROBE_REQUIRED | Warning | **OkButNoFiscalNumber** | disc+cls |
| 13 | `RemoteAuthStatus` (TLS Unauth/PermDenied) | Y | ProbeRequired | ErrorRetryable | ProbeRequired | STAGE_SEND_PROBE_REQUIRED | Warning | **RemoteStatus** | disc+cls |
| 14 | `NoResponse(cause)` — non-wrapper overlay (Transport) | Y | TransientRetry | ErrorRetryable | TransientRetry | STAGE_SEND_TRANSIENT_RETRY | Warning | — | cls |
| 15 | `NoResponse(cause)` — **WrapperBug overlay** (Internal/NotFound/QueryNotSupported/FiscalIdMismatch) | Y | TransientRetry | ErrorRetryable | **WrapperBug** | STAGE_SEND_WRAPPER_BUG² | Critical | — | **overlay** (verbatim) |
| 16 | `PreconditionFailed` (preflight) | no³ | TransientRetry | ErrorRetryable | TransientRetry | STAGE_SEND_TRANSIENT_RETRY | Warning | — | cls (defensive) |
| 17 | `SigningFailed` (preflight) | no³ | WrapperBug | ErrorRetryable | WrapperBug | STAGE_SEND_WRAPPER_BUG | Critical | — | cls (defensive) |

¹ The `Sent` arm's audit is `STAGE_SEND_RESULT`/`Info`, emitted by `append_stage_send_result_audit`'s
  `Sent` match — not a `RoutingDecision` field.
² `ServerFiscalIdMismatch` preserves its DISTINCT `STAGE_SEND_FISCAL_ID_MISMATCH` audit (row 15 carries
  whatever `route_dps_error` produced — `ov.clone()`), not the generic `STAGE_SEND_WRAPPER_BUG`.
³ `NotStarted` preflight leaves cannot reach the post-wire projection (`submit_authorized` returns only
  `Started`). Handled for totality; documented unreachable.

## Behaviour-neutrality (vs today's `project_decision_from_evidence`)

Every reachable row reproduces today's projected `decision` byte-for-byte on the OBSERVED fields
(audit event/severity/retry_class/node_flip/probe_hint; trace retry_class/probe_hint/SFN; returned
`StageSendOutcome`). Verified: `write_path_dps_error_routing.rs` (22/22, incl. the wrapper fx06–fx09),
`stage_send`+`error_routing` lib modules (57/57), and the baseline pins (`rp4b_2_classify_graph_pin`,
`cs3_evidence_matrix_conformance`, `cs3_c_db_classifier_storage_roundtrip`, `pin_d_section_4_6`,
`apply_plan`, `submit_authorized` — 17/17) all GREEN.

The two divergent leaves the old fn special-cased are now STRUCTURAL:
- **RemoteAuthStatus** (row 13): old arm rebuilt ProbeRequired/RemoteStatus from `legacy=TransientRetry`;
  now direct from `cls=ProbeRequired`. (Subsumes the old F3 pin `f3_remote_status_*`.)
- **UnknownStatus** (row 8): old arm rebuilt TransientRetry from `legacy=Decode/ProbeRequired` for `-17`;
  the `-4` case fell through `_ => legacy=TransientRetry`. Both legacy sources now unify under
  `cls=TransientRetry`. (Subsumes the old F3 pin `f3_unknown_nonzero_*`.)

The only intentional forward-looking addition is `ProbeReason::SubmittedUnknown` (row 8′), emitted only
once Pin 3 flips `routing_for_indeterminate(UnknownStatus) → ProbeRequired`. At Pin 2 the classifier
still routes `UnknownStatus → TransientRetry`, so row 8′ is dormant (pre-wired, cannot be unit-pinned yet
because `ClassifiedOutcome` is sealed — Pin 3 will pin it).

## Pins

- **New:** `wire_decision_from_locks_authority_tuple` — the central-change guard (rev4 §4-step-2; none
  existed). Locks rows 8, 13 (divergent), 4 (reject+flip), and 15 (wrapper overlay incl. FiscalIdMismatch
  + the Transport non-application). Two revert-canaries proven to bite (overlay disabled → RED;
  RemoteAuthStatus arm naïve-collapsed → RED).
- **Removed (subsumed):** the 2 F3 tests (`f3_remote_status_*` / `f3_unknown_nonzero_*`), whose behaviour
  is now the RemoteAuthStatus + UnknownStatus rows of the new pin — rev4 §8. (CS-1 re-anchor batched at
  Pin 6.)
- **Untouched (this pin):** the raw legacy-vs-classifier drift-pin (`grpc.rs`) — that's Pin 4.
