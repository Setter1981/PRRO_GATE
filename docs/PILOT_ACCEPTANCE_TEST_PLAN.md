# Pilot Acceptance Test Plan

**Status:** draft methodology  
**Date:** 2026-04-21  
**Scope:** controlled pilot readiness for the Maria 304 to PRRO Gateway to DPS sandbox contour  

This document defines the proposed testing method before a live pilot. It is a
test plan, not evidence that the pilot has already passed.

## Goal

Prove that the contour:

```text
Maria 304 driver -> PRRO Gateway -> signing provider -> DPS sandbox
```

produces fiscal documents that are semantically equivalent to WebCheck output
for the same business operations, and that the gateway correctly preserves
shift state, LND sequencing, idempotency, offline state, audit trail, transport
trace, and recovery behavior.

## Principles

- Use DPS sandbox first. Do not start with production DPS.
- Use one fiscal number first, then repeat on two fiscal numbers.
- Prove online mode before offline mode.
- Prove the contour before building the Windows installer.
- Compare fiscal semantics, not byte-for-byte XML.
- Preserve evidence for every document: raw input, canonical command, generated
  XML, signing metadata, DPS response, database state, audit, and traces.
- Any mismatch in totals, taxes, payment split, UKTZED, excise marks, or Z-report
  totals is a no-go until explained and fixed.

## Phase 0 - Baseline And Environment

Objective: start from a known-good development or release-candidate state.

Required checks:

- Use a clean RC branch or explicitly record unrelated local changes.
- Select exactly one signing and transport execution path for the test run.
- Configure DPS sandbox endpoint.
- Configure Maria 304 driver, gateway, signer, tokens, and ports.
- Create a fresh SQLite database.
- Apply all migrations without manual SQL intervention.
- Confirm that production-like configuration does not use development stubs.
- Verify gateway health endpoints.
- Verify signer or sidecar health.
- Verify Maria 304 driver starts and can reach the gateway.
- Verify `statusRro` and `infoRro` for the selected fiscal number.

Recommended checks:

```bash
pytest -q
cargo test -p maria304_driver
```

Exit criteria:

- Python and relevant Rust tests are green.
- Gateway, signer, and Maria driver start reproducibly.
- The selected fiscal number is visible through DPS sandbox status calls.
- A clean database snapshot is available before acceptance execution.

## Phase 1 - WebCheck Dataset Collection

Objective: build a realistic test pool from existing WebCheck data.

Recommended extraction tool:

```bash
python3 scripts/export_webcheck_samples.py "D:/WebCheck" --limit 200 --xml-limit 500 --selection-quota 3
```

The script accepts WebCheck SQLite databases (`.db`, `.sqlite`, `.sqlite3`) and
standalone XML archives (`.xml`). By default it writes JSON samples under
`var/webcheck_samples/webcheck_export_<timestamp>/`, which is intentionally not
versioned. Use `--no-xml` only for sanitized metadata exports; acceptance runs
should preserve XML for semantic comparison. It also creates `selected/` and
`selected_manifest.json`, selecting samples across all discovered databases/XML
archives for categories such as plain sale, non-excise sale, excise sale,
UKTZED sale, discount, mixed payment, return, service in/out, Z-report,
offline, DB-backed, and XML-backed samples.

For every source receipt, capture:

- WebCheck receipt id.
- Fiscal number.
- Shift identifier or local document number, if available.
- Operation type: sale, return, service in, service out, X-report, Z-report.
- Goods: name, quantity, unit price, line sum.
- Discounts and surcharges.
- Tax group or VAT code.
- UKTZED code.
- Excise marks.
- Payment split: cash, card, mixed payment.
- Rounding and change.
- Receipt total.
- Original receipt reference for returns.
- WebCheck XML, JSON, or raw payload, if available.
- WebCheck DPS status or fiscal response, if available.

Minimum dataset:

- Simple sale without excise.
- Multi-line sale.
- Cash payment.
- Card payment.
- Mixed payment.
- Line-level discount.
- Receipt-level discount.
- Rounding case.
- Return receipt.
- Service in.
- Service out.
- Alcohol or tobacco receipt with excise marks.
- Goods with UKTZED.
- Multiple tax groups.
- Z-report after a representative set of receipts.

Exit criteria:

- The dataset contains both ordinary and regulated goods.
- Expected fiscal totals are known before gateway execution.
- Every receipt has enough source data to explain a mismatch.

## Phase 2 - Semantic Comparison Rules

Objective: define comparison before execution, so acceptance is not subjective.

Compare strictly:

- Operation type.
- Number of lines.
- Quantity, unit price, line sum.
- Receipt total.
- Payment totals and payment split.
- Tax group and tax amount.
- UKTZED.
- Excise marks.
- Discounts and surcharges.
- Rounding and change.
- Return linkage.
- Z-report totals.
- DPS final state: accepted, rejected, retryable, or manual recovery.

Do not require byte-for-byte equality for:

- Request id.
- Transport id.
- Local generated UUIDs.
- Timestamp values when they are expected to differ.
- Signature bytes.
- XML attribute order, if fiscal semantics and DPS acceptance are unchanged.

Exit criteria:

- Each difference is classified as either an acceptable technical difference or
  a fiscal mismatch.
- No fiscal mismatch is allowed into live pilot.

## Phase 3 - Online Contour With One Fiscal Number

Objective: prove the full online fiscal lifecycle for one fiscal number.

Scenario:

1. Start from a fresh database.
2. Onboard one fiscal number.
3. Run `statusRro` and `infoRro`.
4. Open shift.
5. Run service-in operation.
6. Replay the WebCheck-derived sale dataset through Maria 304.
7. Replay the return dataset through Maria 304.
8. Run service-out operation.
9. Run X-report if supported as read-only.
10. Close shift with Z-report.

For every document, verify:

- Ingress row reaches a terminal expected state.
- Fiscal document reaches `ACK`.
- LND increases strictly and has no duplicates.
- Generated XML is archived in `document_files`.
- Payload hash is stable and explainable.
- Protocol trace exists.
- Transport trace exists.
- Audit log has the expected event sequence.
- There are no unexpected `SENT`, `KVT*`, `ERROR_RETRYABLE`,
  `REQUIRES_MANUAL_RECONCILIATION`, or `OFFLINE_LOCAL_ACK` leftovers.

Exit criteria:

- Every online document receives final DPS sandbox `ACK`.
- Semantic diff against WebCheck is zero.
- Z-report totals match the accepted receipt set.

## Phase 4 - Maria 304 Native Path

Objective: prove the real POS path, not only direct REST shortcuts.

For the first stand, real 1C is not required. Use a Windows-side 1C behavior
emulator that reads the selected receipt JSON pool and performs the same call
sequence that 1C would perform against the Maria/OLE compatibility layer.
The extracted call contract is documented in
`docs/ONE_C_MARIA_EMULATOR_CONTRACT.md`.

The emulator must provide operator-visible output while it runs:

- Console progress line for each scenario and receipt.
- Current receipt id, category, fiscal number, operation type, and total.
- Current call name and short argument summary.
- Result status, elapsed time, and error text if any.
- Running counters: total, passed, failed, skipped.
- Path to the current transcript and artifact directory.

Before positive replay, normalize the selected WebCheck pool for the OLE
contract:

- each return receipt must include the original receipt number that will be
  sent through `SetReturnCheckNumber(...)` or `SetReturnCheckNumberStr(...)`;
- each regulated line must have explicit `alcohol` and `cigarettes` flags,
  because the supplied 1C algorithm branches on those fields and they cannot be
  inferred safely from OLE metadata alone;
- samples without return linkage or regulated-goods classification are excluded
  from the positive pool and used as negative or manual-mapping cases.

Example console output:

```text
[001/042] sell_with_excise webcheck__ksef_123 FN=4000162280 total=315.00
  -> OpenShift skipped: already open
  -> ProcessCheck goods=2 payments=1 excise=2 uktzed=1
  <- OK 184 ms document_id=doc_... state=ACK

[002/042] z_report webcheck__ksef_140 FN=4000162280
  -> CloseShift
  <- FAIL 91 ms error=OFFLINE_BACKLOG_NOT_SYNCED
```

The emulator must also write machine-readable logs:

- `transcript.jsonl`: one line per external call.
- `summary.json`: totals by category, operation, fiscal number, and source DB/XML.
- `artifacts/`: raw Maria frames, request/response payloads, and any gateway ids
  returned by the contour.

Minimal `transcript.jsonl` event:

```json
{
  "ts": "2026-04-21T12:00:00.000Z",
  "run_id": "pilot-smoke-001",
  "scenario": "sell_with_excise",
  "receipt_id": "webcheck__ksef_123",
  "fiscal_number": "4000162280",
  "category": "sell_with_excise",
  "call": {
    "method": "ProcessCheck",
    "args_summary": {
      "goods_count": 2,
      "payments_count": 1,
      "has_excise": true,
      "has_uktzed": true,
      "total": "315.00"
    }
  },
  "result": {
    "ok": true,
    "elapsed_ms": 184,
    "return_value": "ACK",
    "error": null
  },
  "artifacts": {
    "raw_frames": "artifacts/receipt-123.frames.jsonl",
    "gateway_document_id": "doc_..."
  }
}
```

For every replayed command, preserve:

- Raw Maria frames.
- Maria session state.
- Request sent to `/v1/ingress/maria304`.
- Canonical JSON accepted by the gateway.
- Generated DPS XML.
- DPS response.
- Related database rows.

Verify:

- CP866 and Cyrillic goods names survive the path.
- UKTZED is preserved.
- Excise marks are preserved.
- Raw frames are stored for audit.
- Idempotency key is stable.
- Replaying the same command does not create a duplicate fiscal document.
- Maria receives a response shape it can map back to the POS flow.

Exit criteria:

- Maria 304 path produces the same fiscal semantics as the expected WebCheck
  dataset.

## Phase 5 - Negative Fiscal Tests

Objective: prove that legally dangerous operations are rejected before signing
or transport.

Required negative cases:

- Sale without an open shift.
- Duplicate shift open.
- Z-report with pending offline backlog.
- Return without valid original receipt reference, if the selected mode requires
  return linkage.
- Missing or invalid excise mark.
- Missing UKTZED where it is mandatory.
- Invalid tax group.
- Payment total not equal to receipt total.
- Cash limit violation.
- Reused idempotency key with different payload.
- Channel switch attempt during an active shift.

Exit criteria:

- Rejection happens before signing and transport.
- Canonical error is explicit.
- Database state remains recoverable and legally unambiguous.

## Phase 6 - Offline With One Fiscal Number

Objective: prove offline lifecycle and later synchronization.

Scenario:

1. Obtain or seed offline code range by the accepted test procedure.
2. Enter offline mode.
3. Run several sales through Maria 304 while offline.
4. Verify responses are `OFFLINE_LOCAL_ACK`, not final `ACK`.
5. Attempt Z-report while offline backlog is pending.
6. Confirm Z-report is blocked.
7. Return online.
8. Run offline synchronization.
9. Confirm all offline documents receive final DPS sandbox `ACK`.
10. Run Z-report after backlog is empty.

Verify:

- Offline fiscal numbers are unique.
- Offline limits are enforced.
- Offline documents remain distinguishable from final DPS-accepted documents.
- Synchronization preserves ordering.
- MAC chain is not broken after offline replay.
- No pending offline backlog remains before Z-report.

Exit criteria:

- Offline receipts are locally acknowledged only.
- Synchronization finishes with final DPS ACK.
- Z-report is allowed only after offline backlog is synchronized.

## Phase 7 - Restart And Recovery

Objective: verify behavior around crashes, retries, and partial progress.

Test scenarios:

- Restart after `PREPARED`.
- Restart after `SIGNED`.
- Restart after `SENT`.
- DPS timeout.
- Retryable transport error.
- DPS rejection.
- Crypto sidecar unavailable.
- Gateway restart during active shift.
- Maria driver reconnect.
- Backup integrity failure leading to stop mode.

Verify:

- No duplicate fiscalization.
- Idempotency is preserved.
- Retry does not corrupt LND or MAC chain.
- Manual recovery state is visible when automatic recovery is not safe.
- Stop mode blocks fiscal operations after integrity failure.

Exit criteria:

- Each interrupted document either completes or lands in an explicit recoverable
  or manual state.

## Phase 8 - Two Fiscal Numbers

Objective: prove multi-FN isolation after the one-FN contour is stable.

Sequential scenario:

- Run a short full lifecycle on fiscal number A.
- Run the same lifecycle on fiscal number B.

Limited parallel scenario:

- Open shifts on both fiscal numbers.
- Run several sales on each.
- Put fiscal number A through offline flow while fiscal number B remains online.
- Close shifts independently.

Verify:

- LND is independent per fiscal number.
- Shift state is independent.
- Channel lock is independent.
- Offline range and session state are independent.
- Audit and trace records can be filtered by fiscal number.
- MAC chain does not cross fiscal numbers.

Exit criteria:

- Fiscal number A and fiscal number B do not affect each other's state.
- Totals, states, and Z-reports are correct per fiscal number.

## Phase 9 - Operational Acceptance

Objective: prove that the contour can be operated on a real point of sale.

Verify:

- Health, readiness, and startup endpoints.
- Ops summary.
- Metrics.
- Logs do not expose secrets.
- Backup creates a valid SQLite copy.
- Retention does not delete fiscal evidence required for audit.
- Rollback procedure to WebCheck is documented.
- Key or JKS replacement procedure is documented.
- Database and backup transfer procedure is documented.

Exit criteria:

- Operator can see current status.
- Engineer can reconstruct every test receipt from stored evidence.
- Secrets are not written to logs.
- Rollback path is practical and rehearsed.

## Phase 10 - Windows Installer Acceptance

The Windows installer should be created after the contour is proven. It should
package a stable deployment shape, not discover it.

Scenario:

1. Start from a clean Windows machine.
2. Install the package.
3. Verify Windows service registration.
4. Verify service start and stop.
5. Reboot the machine.
6. Verify autostart.
7. Onboard a fiscal number without manual SQL.
8. Check gateway, signer, and Maria driver health.
9. Run a short DPS sandbox smoke: shift open, one sale, Z-report.
10. Verify log, database, backup, and key paths.
11. Verify upgrade behavior.
12. Verify uninstall behavior does not remove fiscal archive without explicit
    operator confirmation.

Exit criteria:

- Installation is reproducible.
- Service survives reboot.
- Fiscal smoke passes through sandbox.
- Operational artifacts are stored in expected locations.

## Final Go Criteria For Live Pilot

Live pilot can start only if:

- The WebCheck-derived dataset passes through Maria 304 to DPS sandbox.
- Semantic diff against WebCheck is zero for fiscal fields.
- Online lifecycle passes.
- Offline lifecycle passes.
- Two fiscal numbers pass at least sandbox acceptance.
- No pending documents remain in `SENT`, `KVT*`, `ERROR_RETRYABLE`,
  `REQUIRES_MANUAL_RECONCILIATION`, or `OFFLINE_LOCAL_ACK`.
- Fresh Python and relevant Rust tests are green.
- Windows installer smoke is green.
- Rollback to WebCheck is documented and rehearsed.
- Known gaps are explicitly accepted as out of pilot scope.

## No-Go Conditions

Any of the following blocks live pilot:

- Difference in receipt total.
- Difference in payment split.
- Difference in tax group or tax amount.
- Missing or changed UKTZED.
- Missing or changed excise mark.
- Incorrect return linkage.
- Incorrect Z-report totals.
- Duplicate LND or skipped LND not explained by accepted fiscal behavior.
- Duplicate fiscalization on retry.
- Offline document treated as final DPS ACK before synchronization.
- Pending offline backlog while allowing Z-report.
- Missing audit or trace for an accepted fiscal document.
- Production-like configuration using stub transport or passthrough signing.
