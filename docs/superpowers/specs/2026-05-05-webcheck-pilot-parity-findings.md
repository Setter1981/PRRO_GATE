# WebCheck Pilot-Parity Findings

Status: final
Date: 2026-05-05
Authors: docs-only review pass during M2/W3-C3
Source: `docs/webcheck_reverse/` (decompiled WebCheck binaries)

## Executive summary

A reverse-engineering pass over the WebCheck reference client
during M2/W3-C3 review revealed seven pilot-readiness gaps between
our Rust gateway and the WebCheck deployment that pilot operators
currently run.  None of them block M2/W3 close-out per the
amended ADR-M2-2 + W3 sign-off gate, but each requires an explicit
decision before the M2 → pilot scope review.

This doc consolidates the findings, the per-finding bd issue, and
the pilot-decision matrix in one place.  Per-section evidence
references point at decompiled WebCheck source; per-section bd
ids carry the actionable acceptance criteria.

## DPS RPC parity matrix

| RPC | Canonical proto (M2 W3 scope) | WebCheck TaxGrpc | M2 status | Pilot decision |
|---|---|---|---|---|
| `sendChkV2` | ✅ | ✅ | implemented | mandatory |
| `lastChk` | ✅ | ✅ | implemented | mandatory |
| `ping` | ✅ | ✅ | implemented | mandatory |
| `statusRro` | ✅ | ✅ | implemented | mandatory |
| `infoRro` | ✅ | ✅ | implemented | mandatory |
| `sendChk` (v1) | ❌ | ✅ | deferred | needed IFF pilot uses `apiver=1` |
| `delLastChk` | ❌ | ✅ generated; not on COM `IClient` | deferred | recovery/admin ADR pending |
| `delLastChkId` | ❌ | ✅ generated; not on COM `IClient` | deferred | recovery/admin ADR pending |

Tracking: **PRRO_GATE-0ps** (P1).

Evidence:
- `docs/webcheck_reverse/TaxGrpc/Com.Programika.Rro.Ws.Chk/ChkIncomeService.cs:15,257`
- `docs/webcheck_reverse/TaxGrpc/Com.Programika.Rro.Ws.Chk/CheckRequestId.cs:14`
- `docs/webcheck_reverse/TaxGrpc/TaxGrpc/IClient.cs:7`
- `docs/webcheck_reverse/TaxGrpc/TaxGrpc/Client.cs:57,114`
- `docs/webcheck_reverse/WebCheckMain/WebCheck/All.cs:20`
- `rust/prro/proto/fiscal_server.proto:8`

## TLS / deadline operational requirements

WebCheck loads a CA bundle PEM file
(`C:\ProgramData\WebCheck\prro-tax-gov-ua-chain.pem`) and
constructs an explicit `SslCredentials` for the gRPC channel
(`TaxGrpc/Client.cs:34`).  WebCheck also sets a per-call deadline
via `CallOptions(... DateTime.UtcNow.AddMilliseconds(timeout))`
(`Client.cs:137`).

Our W3 transport:

- **Deadline (✅ in scope, fixed in 580ed20):** `GrpcDpsChannel`
  now sets `tonic::Request::set_timeout(self.request_timeout)` on
  every request, writing the `grpc-timeout` HTTP/2 metadata
  header.  C4 mock asserts the metadata.
- **TLS CA bundle (⏳ open):** currently relies on system TLS
  roots.  A configurable CA-bundle path is required for
  production parity.  Tracking: **PRRO_GATE-k54** (P1).

## WebCheck retry / recovery policy (M3 scope)

WebCheck `SubmitPtr.cs:50` implements policy on top of typed-error
mapping:

- status `-3` → retry with backoff
- status `-15` (no open shift) → recovery via `lastChk` to
  reconcile FN state before next submit
- status `0` (UNKNOWN/missing field) → decode + retry with
  reconciliation
- status `-16` (offline-id mismatch) → offline-pool
  reconciliation, NOT plain retry

Our M2 W3 TYPES these errors (per W0-1 mapping rules) but does
NOT implement the policy.  The policy belongs in M3
`services::write_path`, not in the transport layer.  Tracking:
**PRRO_GATE-6bj** (P1, M3-targeted).

## Offline lifecycle gap

WebCheck implements:

- offline-document pool with per-FN limits
  (`NumbersOfflineUse.cs:69`)
- `OfflineID()` generation
- `CloseOfflineDoc` / sync-back-online flow
  (`ClassFiscal.cs:1532`)
- 24h shift guard interacting with offline timing
  (`All.cs:188`)

Our gateway has M1 schema for offline tables + the legacy
PRRO_GATE-er6 (Python-era), but no Rust-side offline lifecycle.
For pilots that require offline operation (legally mandated for
UA fiscal endpoints — operators MUST be able to issue receipts
when DPS is unreachable, up to the offline-code limit), the
gateway is NOT pilot-ready without a real offline lifecycle.

Tracking: **PRRO_GATE-gx2** (P1).  Cross-linked with -er6, -6bj,
-0ps.

## WebCheck COM / 1C surface gap

WebCheckServer exposes a 19-method COM/OLE surface to 1C and
other Windows clients
(`WebCheckServer/vk_WebCheckServer.cs:20`, `:245`).  If a pilot
operator runs 1C against WebCheck COM, our Rust gateway needs
either a compatibility shim or an out-of-band integration path
(REST / signed-doc file drop / etc.).

Tracking: **PRRO_GATE-iap** (P2; bumps to P1 if pilot survey
identifies a dependent operator).

## Print / export / check URL gap

WebCheck `PrintExportCheck.cs:121` builds receipt verification
URLs (the official cabinet.tax.gov.ua links customers scan to
verify their receipt) and drives print/export to multiple
formats.

Our M5 milestone is the home for receipt rendering; M2/M3 do not
ship this.  Pilot operators will likely need at least the
verification URL + a basic ESC-POS or PDF rendering before
sign-off.

Tracking: **PRRO_GATE-3a8** (P2).

## Recommended bd follow-ups (consolidated)

| bd id | Priority | Title | Milestone target |
|---|---|---|---|
| PRRO_GATE-0ps | P1 | DPS proto drift (sendChk / delLast*) | M2 W3 sign-off gate |
| PRRO_GATE-k54 | P1 | DPS TLS CA bundle support | M2 W3 follow-up or M3 |
| PRRO_GATE-6bj | P1 | M3 write_path retry/recovery policy | M3 |
| PRRO_GATE-iap | P2 | WebCheck COM/1C 19-method compat | pilot survey → milestone |
| PRRO_GATE-gx2 | P1 | Offline lifecycle parity | M3 or M_offline |
| PRRO_GATE-3a8 | P2 | Print/export/check URL parity | M5 |

All linked `discovered-from PRRO_GATE-82j` (M2 epic).

## What does NOT block M2 W3

- The 5 RPCs in W3 scope (`sendChkV2`, `lastChk`, `ping`,
  `statusRro`, `infoRro`) cover the production submit + recovery
  query surface.  W3 implementation matches the canonical proto.
- Typed-error mapping rules are in place
  (Authorization / Transport / Decode / Server / NotFound /
  ServerFiscalIdMismatch / QueryNotSupported / Internal).
- gRPC `grpc-timeout` deadline is set per request (fixed in
  580ed20).
- ByServerFiscalNo semantic is canonical (PRRO_GATE-5js,
  implemented in C3 as the trait default body).
- The 3 deferred RPCs and the 4 adjacent gaps are documented +
  tracked + cross-linked from the W3 sign-off gate; they do not
  cause silent drift.

## Verify command

```bash
test -f docs/superpowers/specs/2026-05-05-webcheck-pilot-parity-findings.md && \
  grep -E '^Status: (final|deferred)$' docs/superpowers/specs/2026-05-05-webcheck-pilot-parity-findings.md
```

Expected output: `Status: final`.
