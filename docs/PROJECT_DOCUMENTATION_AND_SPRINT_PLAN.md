# PRRO Gateway: Project Documentation And Sprint Plan

## 1. Purpose

PRRO Gateway is a local edge gateway for the Ukrainian software cash register market. It accepts fiscal commands from existing POS/front-office systems, normalizes them into one canonical model, persists every command in a local SQLite hot store, and executes fiscal write-path operations with recovery, audit, offline handling, and transport abstraction.

The system must not be treated as a generic receipt adapter. It handles fiscal operations with legal and operational consequences under Ukrainian RRO/PRRO regulation.

Primary goals:

- support multiple ingress protocols: Checkbox-style REST, WebCheck XML-RPC, Maria TCP;
- isolate external protocol compatibility from the internal canonical model;
- process operations through a durable, auditable, single-writer write-path per fiscal number;
- support online fiscalization through configured backend/transport profiles;
- support legally constrained offline operation with reserved fiscal numbers;
- preserve local archive, audit trail, traces, and recovery state;
- prepare a production path for DPS fiscal server integration.

Checkbox, WebCheck, and Maria are ingress compatibility protocols. The target production egress architecture is `local PRRO core -> direct DPS submission`; `CHECKBOX_REST_TRANSPORT` is compatibility/migration only.

## 2. Regulatory Baseline

This project targets the Ukrainian PRRO domain. The engineering baseline is derived from:

- Law of Ukraine No. 265/95-VR on RRO/PRRO usage: `https://zakon.rada.gov.ua/laws/show/265/95-%D0%B2%D1%80`
- Ministry of Finance Order No. 317, PRRO registration/use procedure: `https://zakon.rada.gov.ua/laws/show/z0635-20`
- Ministry of Finance Order No. 317, offline fiscal number ranges: `https://zakon.rada.gov.ua/laws/show/z0636-20`
- Ministry of Finance Order No. 13, fiscal receipt document form/content: `https://zakon.rada.gov.ua/go/z0220-16`
- Ministry of Finance Order No. 1057, electronic control tape and data transfer procedures: `https://zakon.rada.gov.ua/go/z1744-12`
- DPS Electronic Cabinet API entry point: `https://cabinet.tax.gov.ua/help/api.html`

Regulation can change. Before production release, the team must re-check the current legal text and DPS technical documentation.

### DPS Submission Channels

The product must explicitly support two DPS-side submission channels for checks, Z-reports, and related PRRO service documents:

> **Rust gateway channel-taxonomy mapping (2026-05-16).**  The Rust gateway formalises these two channels under the names **WebCheck / gRPC channel** (M3a + M3b W7-W9a-W10 target — corresponds to the project's `DPS_PRRO_FISCAL_SERVER` family below) and **DFS HTTP / XML channel** (future implementation, NOT in M3b — corresponds to a `/fs/cmd` + `/fs/doc` + `/fs/pck` HTTPS endpoint family per the reference `PRRODPS.DFS` codebase, with offline numbering via `OfflineSessionId.localOfflineNum.controlNumber` instead of a pre-fetched code pool).  See `docs/superpowers/plans/2026-05-14-m3b-implementation.md` §"DPS Channel Taxonomy" for the channel comparison.  The exact mapping between the Python-era channel labels and the Rust channel taxonomy is left for the runtime-composition task that constructs the production `DpsChannel` backend.  Maria 304 is NOT a DPS channel — it is an ingress / POS adapter on the same boundary as REST / XML-RPC / Maria-TCP shells.

1. `DPS_UNIFIED_WINDOW`
   - Ukrainian name: `Єдине вікно подання електронної звітності`;
   - project transport meaning: reporting-window / XML-document submission contour;
   - current project analogue: `DPS_PRRO_XML_UNIFIED_WINDOW`;
   - must be treated as a first-class production channel, not as a fallback hidden inside another transport.

2. `DPS_PRRO_FISCAL_SERVER`
   - public endpoint family: `https://prro.tax.gov.ua:443`;
   - additional/legacy endpoint mentioned by DPS documentation: `https://prro2.tax.gov.ua:443`;
   - developer test endpoint mentioned by DPS documentation: `https://cabinet.tax.gov.ua:9443`;
   - project transport meaning: direct PRRO fiscal server API contour;
   - current project analogue: `DPS_PRRO_GRPC_ECABINET`, but the final implementation name should be reviewed after confirming the current DPS API protocol.

These channels must be represented in configuration, routing, audit, transport traces, readiness, and acceptance tests. While a shift is open, switching between `DPS_UNIFIED_WINDOW` and `DPS_PRRO_FISCAL_SERVER` is strictly forbidden (Rust gateway: no channel switch between WebCheck/gRPC and DFS HTTP/XML while a shift is open — see `docs/LEGAL_INVARIANTS.md` INV-05). A receipt submitted through one DPS channel must not be retried through the other channel during the same open shift.

## 3. Legal Engineering Invariants

These rules drive architecture and sprint acceptance.

1. One fiscal number must have one logical write-path writer.
2. A shift must be opened before fiscal sale/return/service operations.
3. One PRRO/fiscal number must not have two active shifts.
4. Channel switch during an active shift is forbidden.
5. Idempotency is mandatory: one business operation must not create two fiscal documents.
6. Offline mode is allowed only when the fiscal server is unavailable and only within legal limits.
7. Offline duration must not exceed 36 hours continuously and 168 hours per calendar month.
8. Offline operation requires a pre-issued fiscal number range from the fiscal server.
9. One offline fiscal number must be used for only one electronic document.
10. An offline local receipt is not a final DPS-registered receipt until it is transmitted and acknowledged by the fiscal server.
11. Offline documents must be retained locally until fiscal server delivery confirmation.
12. Z-report / fiscal daily report must not bypass unsent offline documents for the relevant period.
13. Excise goods must carry required UKTZED and excise mark data when legally required.
14. Production mode must not use passthrough signing or mock transports.
15. Every state transition must be recoverable or explicitly marked for manual reconciliation.
16. Receipt submission channel is part of the fiscal route and must be auditable.
17. Switching between `DPS_UNIFIED_WINDOW` and `DPS_PRRO_FISCAL_SERVER` during an open shift is strictly forbidden.
18. Channel failover can be considered only outside an active shift, or after a controlled shift close/open procedure with explicit operator decision, audit event, and idempotency proof.

## 4. Current Project State

Current package version in code/config: `1.4.1`.

Implemented foundations:

- Pydantic canonical models and JSON schemas;
- SQLite hot store schema and migration runner;
- repository layer for inbox, documents, shifts, offline ranges, audit, traces, outbox;
- Checkbox REST, WebCheck XML-RPC, Maria TCP adapters;
- Checkbox REST compatibility transport implementation;
- DPS transport stubs;
- staged write-path worker;
- idempotency through `ingress_inbox.idempotency_key`;
- channel lock enforcement in worker;
- offline session/range model and 36/168 time checks;
- excise duplicate mark protection;
- reconciliation service for pending transport states;
- FastAPI REST runtime;
- XML-RPC and Maria shell entry points;
- startup supervisor, health endpoints, metrics, alerts, structured logs;
- crypto provider seam with passthrough and sidecar client/provider;
- admin endpoints for manual document retry and crypto breaker reset;
- extensive pytest gate suite.

Baseline test state (2026-04-15, Sprint 10 wave 2):

- `pytest -q` result: `586 passed, 0 failed`;
- ФСКО protocol complete: `<D>/<S>` discounts, `<L>` comments, `<EPZ>` Z-report, `<E>` full attrs, `<CA>`, `<CZD>`;
- all P0 legal gaps closed: `OFFLINE_LOCAL_ACK`, `OfflineSyncService`, Z-report offline backlog guard *(Python-era — blanket-blocker shape; superseded by the M3b W10 ONLINE-vs-OFFLINE-distinguished policy in the Rust gateway, see `docs/OFFLINE_SHIFT_CLOSE_DECISION.md` §0)*, receipt validator;
- full fiscal lifecycle E2E test: SHIFT_OPEN → SELL → SELL → SERVICE_IN → SERVICE_OUT → RETURN → Z_REPORT.

## 5. High-Level Architecture

### 5.1 Runtime Components

Ingress layer:

- REST endpoint: `/v1/ingress/checkbox`;
- XML-RPC shell: `scripts/run_xmlrpc.py`;
- Maria TCP shell: `scripts/run_maria.py`;
- adapters map raw protocol requests into `CanonicalFiscalCommand`.

Durable accept layer:

- every command is stored in `ingress_inbox`;
- idempotency is enforced by unique `idempotency_key`;
- inbox status machine: `NEW -> PROCESSING -> DONE / ERROR / DEAD`;
- worker leases prevent duplicate processing.

Write-path layer:

- `WritePathWorker` is the fiscal operation executor;
- it performs guard checks, document creation, signature, transport send/offline handling, audit, traces, and final state transition;
- SQLite `BEGIN IMMEDIATE` is used for critical write sections;
- network and crypto calls are kept outside long write transactions.

Transport layer:

- `ProfileAwareTransportRouter` resolves active `transport_profile_id`;
- implemented: Checkbox REST compatibility transport;
- stubbed: DPS PRRO fiscal server and DPS XML Unified Window;
- target production priority: real `DPS_UNIFIED_WINDOW` and `DPS_PRRO_FISCAL_SERVER` transport handlers;
- `CHECKBOX_REST_TRANSPORT` must be treated as a compatibility or migration contour, not as the target direct-DPS egress path.

Crypto layer:

- `PassthroughCryptoProvider` for dev/test;
- `SidecarCryptoProvider` through HTTP sidecar;
- production must require real signing with KEP/seal and must fail startup if passthrough is configured.

Recovery and reconciliation:

- startup supervisor can run reconciliation after phase 1 readiness;
- pending documents can be polled and moved to `ACK`, `REJECTED`, `ERROR_RETRYABLE`, or `REQUIRES_MANUAL_RECONCILIATION`;
- admin retry resets manual documents back to retryable state.

Storage:

- SQLite hot store is the source of truth;
- WAL mode is enabled;
- `quick_check` runs at startup;
- filesystem archive paths are represented in `document_files`;
- sync outbox exists for cloud/archive delivery.

## 6. Core Data Model

Important tables:

- `node_state`: PRRO node/fiscal number state, mode, readiness, offline counters, current shift references;
- `backend_profiles`: logical backend capability profiles;
- `transport_profiles`: technical transport configuration;
- `prro_bindings`: fiscal number routing to backend/transport profiles;
- `ingress_inbox`: durable command intake and idempotency;
- `shifts`: shift lifecycle and channel lock;
- `offline_ranges`: available offline fiscal number ranges;
- `offline_sessions`: offline lifecycle and accumulated limits;
- `fiscal_documents`: fiscal document state machine and canonical payload;
- `fiscal_document_goods`: receipt goods;
- `fiscal_document_payments`: payment/acquiring data;
- `excise_marks`: reserved/sold/returned/voided excise mark registry;
- `document_files`: archive file metadata;
- `sync_outbox`: delivery queue for hub/archive artifacts;
- `protocol_trace_log`: ingress protocol trace;
- `transport_trace_log`: backend/transport trace;
- `audit_log`: operational and fiscal audit events;
- `schema_registry` and `schema_migrations`: schema/version tracking.

## 7. Main Runtime Flows

### 7.1 Online Fiscal Operation

1. Client sends protocol-specific command.
2. Adapter maps command to `CanonicalFiscalCommand`.
3. Command is inserted into `ingress_inbox`.
4. Worker acquires lease.
5. Worker validates backend capabilities, shift state, channel lock, fiscal number state.
6. Worker creates `fiscal_documents` record in `PREPARED`.
7. Worker signs payload through configured crypto provider.
8. Worker sends payload through resolved transport profile.
9. Worker stores transport traces and response files.
10. Document moves to final or pending state.
11. Inbox is marked `DONE` or `ERROR`.

### 7.2 Offline Fiscal Operation

Current intended flow:

1. Node enters offline mode only when fiscal server is unavailable.
2. Worker validates open offline session and legal time limits.
3. Worker allocates a fiscal number from `offline_ranges`.
4. Document is created locally with offline fields.
5. Local receipt can be returned to the POS/customer.
6. Document must later be transmitted to DPS after connectivity is restored.

Required correction:

- local offline success must not be represented as final DPS `ACK`;
- the project needs explicit states and service logic for offline sync.

### 7.3 Shift Lifecycle

1. `SHIFT_OPEN` creates/open-syncs a shift after fiscal success.
2. Active shift creates channel lock:
   - backend profile;
   - transport profile;
   - protocol;
   - integration owner.
3. Sale/return/service operations must match the active channel lock.
4. `SHIFT_CLOSE` closes the active shift after fiscal success.
5. Z-report/fiscal daily report must respect pending/offline document constraints.

### 7.4 DPS Channel Routing

Every fiscal command must resolve to one explicit submission channel before transport execution.

Required routing fields:

- `submission_channel`: `DPS_UNIFIED_WINDOW`, `DPS_PRRO_FISCAL_SERVER`, `CHECKBOX_COMPAT`, or future channel;
- `backend_profile_id`;
- `transport_profile_id`;
- `fiscal_number`;
- `integration_owner`;
- optional `route_key`.

Routing rules:

1. The selected channel is stored with the inbox command and fiscal document.
2. The selected channel is part of the active shift channel lock.
3. Reconciliation uses the original document channel.
4. Any channel change during an active shift is rejected unconditionally.
5. Audit and transport traces must show both the logical channel and concrete endpoint/profile.
6. Offline sync must use the channel policy assigned to the document or fiscal number.
7. Reconciliation and admin retry must not move an in-shift document from one DPS channel to another.

## 8. Required Product Changes

### P0: Legal Correctness

1. Add explicit offline document states:
   - `OFFLINE_LOCAL_ACK`;
   - `OFFLINE_PENDING_SYNC`;
   - `OFFLINE_SYNC_SENT`;
   - `DPS_ACK`;
   - `DPS_REJECTED`;
   - `REQUIRES_MANUAL_RECONCILIATION`.

2. Add `OfflineSyncService`:
   - select unsent offline documents;
   - send in deterministic fiscal/local sequence;
   - mark DPS acknowledgement separately from local receipt creation;
   - retry safely;
   - create transport trace and audit events.

3. Block Z-report/shift close where legally necessary:
   - no fiscal daily report before required offline documents are sent;
   - clear canonical error for blocked operation.

4. Strengthen offline number accounting:
   - persist per-number lifecycle or equivalent immutable audit;
   - detect duplicates after crash;
   - add low-watermark and exhaustion alerts.

5. Add Ukrainian fiscal receipt validator:
   - mandatory fields per document type;
   - seller/PRRO/shift/document metadata;
   - payment and totals consistency;
   - QR/check payload fields;
   - offline markers;
   - excise data requirements.

### P1: Production Fiscal Server Readiness

1. Reuse the persisted signed outbound artifact seam for transport-neutral recovery/offline sync:
   - signed payload content must remain recoverable after restart;
   - offline sync and reconciliation must prefer persisted signed content over Checkbox-shaped `request_payload_json`;
   - pre-migration or unsigned-profile cases must be surfaced explicitly as warning/fallback paths.
2. Implement the first real DPS transport profile end-to-end.
3. Implement the second DPS transport profile after the first contour proves recovery and routing invariants.
4. Add explicit channel routing:
   - per fiscal number;
   - per backend profile;
   - per operation type where legally allowed;
   - no cross-channel fallback while a shift is open.
5. Support DPS service documents:
   - shift open;
   - shift close;
   - offline begin;
   - offline end;
   - Z-report/fiscal daily report;
   - status/polling.
6. Replace dev passthrough crypto in production:
   - real KEP/seal sidecar;
   - certificate/cashier binding;
   - startup validation.
7. Add production configuration profile and startup gates.
8. Add immutable archive bundle and export procedure.

### P2: Operational Hardening

1. Add rate limiting to ingress.
2. Add backup and corruption runbooks plus automated backup job.
3. Add retention policy for audit/trace/archive.
4. Add readiness legal blockers.
5. Add deployment profiles for Windows bare metal, Linux Docker, and VM.
6. Add observability dashboards/metrics.

## 9. Sprint Plan

The sprint plan assumes 2-week sprints after Sprint 0. Team size can be 1-3 engineers. If the team is smaller, keep the sequence and reduce scope per sprint.

### Sprint 0: Baseline And Legal Alignment

Duration: 1 week.

Goal:

- establish a reliable baseline before changing fiscal state machines.

Scope:

- fix date-sensitive offline tests through relative time or clock injection;
- document legal invariants in code docs;
- create acceptance coverage map against legal requirements;
- confirm current schema and code version;
- identify production blockers.

Deliverables:

- green baseline test suite;
- `docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md`;
- `docs/LEGAL_INVARIANTS.md`;
- issue/backlog list for P0/P1/P2.

Acceptance criteria:

- `pytest -q` passes;
- no new behavior change except test fixture stability;
- legal invariants are reviewed by product/legal owner.

### Sprint 1: Offline State Model Correction

Duration: 2 weeks.

Goal:

- stop treating local offline receipt creation as final DPS acknowledgement.

Scope:

- extend `DocumentState` or add offline sync status fields;
- migrate schema safely;
- update write-path offline finalization;
- update REST response naming so local offline success is explicit;
- update tests that expect `ACK` for offline local documents;
- preserve idempotency and offline number allocation behavior.

Deliverables:

- new offline state machine;
- migration;
- updated repository methods;
- updated REST/admin visibility;
- tests for local offline success vs DPS final acknowledgement.

Acceptance criteria:

- online flows still pass;
- offline local operation returns success without claiming DPS ACK;
- replay does not allocate a second offline number;
- no network call occurs inside long SQLite transaction.

### Sprint 2: Offline Sync Service

Duration: 2 weeks.

Goal:

- transmit offline documents to the fiscal server after connectivity is restored.

Scope:

- implement `OfflineSyncService`;
- select eligible offline documents in strict sequence;
- add retry/manual reconciliation states;
- persist transport traces and audit events;
- add metrics for unsynced offline backlog;
- expose ops summary fields.

Deliverables:

- service implementation;
- repository selectors;
- transport interface extension if needed;
- tests for ordered sync, retry, rejection, crash recovery.

Acceptance criteria:

- offline documents are not lost after restart;
- one offline fiscal number maps to one document;
- synced documents reach DPS final state only after transport confirmation;
- failed sync is visible and retryable.

### Sprint 3: Z-Report And Shift Legal Guards

Duration: 2 weeks.

Goal:

- enforce daily fiscal report and shift constraints around offline documents.

Scope:

- add guard that blocks Z-report/shift close where unsynced offline documents must be sent first;
- formalize shift close behavior when pending online documents exist;
- add canonical error codes for legal blockers;
- update reconciliation side effects for shift open/close;
- add tests for all guard paths.

Deliverables:

- guard policy;
- canonical errors;
- shift/Z-report test suite.

Acceptance criteria:

- no Z-report bypasses required offline sync;
- one active shift invariant remains enforced;
- channel lock remains authoritative in worker transaction.

### Sprint 4: Ukrainian Receipt Validation Layer

Duration: 2 weeks.

Goal:

- validate fiscal receipt payloads against Ukrainian PRRO requirements before fiscalization.

Scope:

- create validators for required receipt fields;
- validate totals/payments consistency;
- validate seller/PRRO metadata presence;
- validate QR/check payload fields where applicable;
- add document-type-specific rules;
- keep adapters responsible for mapping, not for legal validation.

Deliverables:

- `validators/ua_receipt.py`;
- test matrix for sale, return, service, cash withdrawal, shift, Z-report;
- structured canonical errors.

Acceptance criteria:

- invalid fiscal payloads are rejected before transport send;
- valid existing test payloads still pass after fixture updates;
- validation failures are auditable and visible through REST response.

### Sprint 5: Excise Compliance

Duration: 2 weeks.

Goal:

- make excise handling legally useful, not only duplicate-safe.

Scope:

- define excise policy config;
- require UKTZED for excise goods;
- require excise mark barcode for alcohol retail cases;
- normalize and validate mark format;
- add return/void behavior for marks;
- add negative tests.

Deliverables:

- excise validation policy;
- additional state transitions for returned/voided marks if needed;
- tests for duplicate, missing, malformed, returned mark scenarios.

Acceptance criteria:

- duplicate marks remain blocked;
- legally required excise fields are enforced;
- returns do not leave marks permanently inconsistent.

### Sprint 6: Production Crypto Gate

Duration: 2 weeks.

Goal:

- ensure production cannot run with dev signing.

Scope:

- add `runtime.environment` or `regulatory.mode`;
- block `crypto.provider=passthrough` in production;
- add startup health failure for missing sidecar URL/cert binding;
- add crypto audit events;
- add certificate/cashier metadata model if required.

Deliverables:

- production config validation;
- crypto readiness signals;
- tests for dev/test/prod modes.

Acceptance criteria:

- dev/test can use passthrough;
- production fails startup without real crypto;
- crypto breaker state is visible in health/ops.

### Sprint 7: Staged DPS Transport Integration

Duration: 2-3 weeks.

Goal:

- add the first real direct DPS submission contour, then stage the second contour without breaking recovery or channel-routing invariants.

Scope:

- study latest DPS API format and test access constraints;
- reuse the persisted signed outbound artifact seam for recovery/offline sync so direct DPS egress does not depend on Checkbox-shaped request payload handling;
- implement the first available real DPS transport handler;
- implement the second DPS transport handler only if endpoint certainty and test access are sufficient; otherwise leave an explicit staged plan;
- add channel routing config and validation;
- support fiscal documents and service documents for the implemented DPS contour(s);
- parse DPS responses/receipts/errors;
- add polling/status behavior;
- map DPS errors to canonical errors;
- prevent all cross-channel retry while a shift is open.

Deliverables:

- verified use of the persisted signed outbound artifact seam for recovery/offline sync;
- `transports/dps_unified_window.py` or equivalent, and/or `transports/dps_prro_fiscal_server.py` for the first implemented contour;
- explicit staged plan for the second contour if it is not implemented in the same sprint;
- profile seed/migration;
- channel routing documentation;
- integration tests with mock DPS server;
- operator config example.

Acceptance criteria:

- transport works through `ProfileAwareTransportRouter`;
- recovery/offline sync no longer depend on Checkbox-shaped `request_payload_json` for direct DPS egress;
- no Checkbox-specific assumptions leak into DPS path;
- retryable vs terminal DPS failures are classified correctly;
- every document records the selected DPS channel;
- reconciliation uses the original channel;
- at least one direct DPS contour works end-to-end before the second contour is considered complete;
- channel switch during an open shift is rejected unconditionally;
- failover, if enabled outside an active shift, is explicit, audited, and idempotency-safe.

### Sprint 8: Archive, Control Tape, And Export

Duration: 2 weeks.

Goal:

- create an audit-grade archive contour for fiscal documents and offline evidence.

Scope:

- define immutable archive layout;
- store canonical request, signed payload, response/KVT, printable receipt, hash metadata;
- add export command for audit/support;
- add retention policy;
- add integrity verification command.

Deliverables:

- archive writer/reader;
- CLI/export script;
- operations documentation.

Acceptance criteria:

- every fiscal document can be reconstructed from DB + archive;
- archive integrity can be checked;
- offline documents remain available until confirmed delivery.

### Sprint 9: Ingress And Operational Hardening

Duration: 2 weeks.

Goal:

- make the edge node safer under real local network conditions.

Scope:

- add rate limiting per source/session;
- add request size limits;
- add structured ingress trace policy;
- add backup job and corruption runbook;
- improve readiness blockers;
- improve graceful shutdown around active worker operations.

Deliverables:

- hardened REST/XML-RPC/Maria ingress;
- backup/corruption docs and scripts;
- expanded health checks.

Acceptance criteria:

- abusive local clients are throttled;
- corrupted DB startup behavior is deterministic;
- health endpoints reflect fiscal readiness, not only process liveness.

### Sprint 10: Pilot Acceptance And Deployment

Duration: 2 weeks.

Goal:

- package a controlled pilot release.

Scope:

- finalize Docker/systemd deployment profiles;
- create pilot runbook;
- create upgrade/rollback procedure;
- run end-to-end pilot acceptance matrix;
- review security and compliance gaps;
- freeze known limitations.

Deliverables:

- pilot release candidate;
- runbook;
- acceptance report;
- deployment checklist.

Acceptance criteria:

- all P0 items are complete;
- P1 items required for selected pilot topology are complete;
- known gaps are explicit and signed off;
- no test failures in mandatory suite.

### Sprint 11: Offline Full Lifecycle

Duration: 2 weeks.

Goal:

- close all remaining offline-state gaps in one vertical slice.

Scope:

- implement API-driven GO_OFFLINE flow (not manual DB seed);
- add E2E test for offline code range request: ASK_OFFLINE_CODES → store → use → exhaust;
- add LND crash+recovery scenario test: crash during offline session, restart, assert LND remains monotonic;
- add channel-failover guard test: channel switch attempt with open shift = REJECTED;
- ensure offline sync test suite covers ordered delivery, retry, rejection, crash recovery.

Deliverables:

- API-driven GO_OFFLINE test;
- offline range request E2E;
- LND crash recovery test;
- channel failover guard test.

Acceptance criteria:

- GO_OFFLINE and GO_ONLINE are triggered through API, not DB seed;
- offline number is requested, stored, and spent atomically — one number per document enforced;
- LND after crash+recovery is strictly monotonic — no gaps, no duplicates;
- channel switch during open shift is rejected unconditionally with explicit error code.

---

### Sprint 12: Fiscal Compliance Completeness

Duration: 1–2 weeks.

Goal:

- close remaining fiscal accuracy gaps for excise goods and cash balance.

Scope:

- complete excise goods E2E pipeline: adapter → write-path → DPS XML (УКТЗЕД + excise mark);
- implement cash balance carry-over between shifts;
- serialize cash balance into shift-open DPS XML payload.

Deliverables:

- excise pipeline E2E test (adapter → serializer);
- cash balance carry-over in shift open/close flow;
- DPS XML includes opening cash balance.

Acceptance criteria:

- excise good with УКТЗЕД and mark passes full pipeline to XML without error;
- attempt to sell excise good without УКТЗЕД = REJECTED before sign;
- opening cash balance of new shift equals closing balance of previous shift;
- DPS XML shift-open payload includes correct cash balance field.

---

### Sprint 13: Production Infrastructure

Duration: 2–3 weeks.

Goal:

- make the system production-deployable.

Scope:

- crypto sidecar: add TLS with mutual auth, graceful shutdown, multi-threaded request handling;
- implement `DPS_UNIFIED_WINDOW` transport handler (`transports/dps_unified_window.py`);
- add ingress rate limiting (REST + XML-RPC);
- add request size limits.

Deliverables:

- production-hardened sidecar with TLS;
- `DPS_UNIFIED_WINDOW` transport with mock DPS integration tests;
- rate limit middleware with 429 response and audit event.

Acceptance criteria:

- sidecar rejects requests without valid client certificate;
- `DPS_UNIFIED_WINDOW` successfully submits a mock fiscal document;
- ingress rejects rate-limit excess with HTTP 429 and audit log entry;
- channel routing config distinguishes DPS_UNIFIED_WINDOW from DPS_PRRO_FISCAL_SERVER.

---

### Sprint 14: Operational Safety And Pilot

Duration: 2 weeks.

Goal:

- operational correctness, data lifecycle, pilot readiness.

Scope:

- SQLite backup job + corruption detection → STOP_MODE trigger;
- retention/purge policy for audit/trace/archive tables (configurable TTL);
- write four missing operational docs: `PROTOCOL_SHAPE_AUDIT.md`, `DPS_TRANSPORT.md`, `OFFLINE_SYNC.md`, `ARCHIVE_POLICY.md`;
- add pytest markers: `unit`, `integration`, `e2e`;
- run full pilot acceptance matrix (see §10).

Deliverables:

- backup job + runbook;
- retention config + purge script;
- four operational docs;
- pytest marker taxonomy.

Acceptance criteria:

- automatic backup runs on schedule and verifies integrity;
- SQLite corruption transitions node to STOP_MODE with visible health signal;
- old audit/trace records are purged according to configured TTL;
- all four docs exist and match current code behavior;
- `pytest -m unit` / `pytest -m integration` / `pytest -m e2e` each select disjoint subsets.

---

### Phase 1.1 Candidate Slice: Key Inspection And PRRO Onboarding

Goal:

- make multi-PRRO operator workflows safer by separating read-only key inspection from actual PRRO binding.

Scope:

- provide an explicit read-only key inspection flow;
- accept a key/container path or upload and password;
- verify that the key opens successfully in the configured crypto stack;
- display extracted key identifier and certificate identity fields without storing the key in the installed-key registry;
- automatically pull or refresh certificate-chain/trust material needed to evaluate the signer and show the resulting trust/readiness status explicitly;
- optionally show discovered `NumFiscal` candidates for the inspected signer;
- keep the actual PRRO bind step separate and explicit;
- let the operator bind the selected PRRO/fiscal number into local routing/profile configuration only after inspection.
- support an optional scheduled bind window (`active_from`, `active_to`) for saved signer-to-PRRO bindings;
- enforce signer usability as the conjunction of certificate validity and local bind-window validity.
- support multiple saved signer/operator bindings for the same PRRO/fiscal number;
- enforce at runtime that an open shift still has exactly one active signer, even if several operators are eligible for that PRRO outside an active shift.
- read cashier-role metadata from DPS `infoRro`, including operator serial, display name, status, and senior-cashier flag;
- surface DPS operator-role metadata in operator tooling instead of relying only on locally entered role labels.

Notes:

- this is a follow-up onboarding slice, not a blocker for the first live DPS smoke;
- WebCheck archive evidence suggests a signed discovery request/response shape with `TaxObjects[].TransactionsRegistrars[]`, so discovery should be modeled as key-backed PRRO lookup rather than manual fiscal-number entry only;
- automatic certificate/chain pull should remain operator-visible: the UI should show whether trust material was loaded successfully instead of silently assuming readiness;
- scheduled binding is an operator-side policy, distinct from the certificate's own `notBefore` / `notAfter`;
- multiple operators may legitimately be configured for one PRRO, but the direct DPS runtime must still respect the fiscal-server constraint that one shift cannot mix signers;
- DPS `infoRro` should be treated as the authoritative source for cashier vs senior-cashier role visibility where available;
- the read-only inspection screen is specifically useful for operators who manage several PRRO/fiscal numbers and often confuse which signer/key is currently selected.

Prioritized follow-up items:

P1:

- display key fingerprint/serial/issuer/validity together with the extracted key identifier on the inspection screen;
- add a single explicit preflight/readiness summary before bind or activation: key opens, trust chain loaded, certificate currently valid, DPS operator metadata available, PRRO discovery/probe status visible;
- block activation/switch of the effective signer binding while a shift is open on that PRRO.
- add explicit database validation tooling: SQLite integrity check, schema/state sanity checks, and detection of broken/missing local signer/PRRO bindings;
- add explicit DPS reconciliation tooling: compare local PRRO state with `statusRro` / `infoRro` / `lastChk` snapshots and surface mismatches before/after incidents;
- add failure-pattern classification for transport and crypto dependencies so the system distinguishes DNS / OCSP-TSP-CMP / TLS / gRPC-unavailable / business reject / uncertain-state cases instead of collapsing them all into “no connection to DPS -> offline”;
- make offline gating depend on classified cause: only genuine transport-unreachability may permit offline fallback, while TLS/crypto/config/business rejects must stay explicit operator-visible errors and uncertain-state cases must route to reconciliation.
- enforce the legal cash-transaction amount limit before signing and transport submission, with a clear operator-facing rejection when the configured/legal threshold is exceeded.
- enforce excise-mark compliance before signing and submission: validate mark format, reject duplicates within the same document, and reject duplicates already sold in local history, with an explicit operator-facing compliance error.
- enforce tax-group correctness before signing and submission: validate that each item/document uses an allowed tax group, and correctly handle combinations that include excise tax rather than assuming a single flat tax-group model.
- scope tax/excise/UKTZED policy groups per PRRO/fiscal number, so different fiscal numbers may legitimately use different effective policy-group sets and constraints.
- allow declaration-oriented excise subgrouping (for example vodka / wine / cigarettes) on top of those per-PRRO policy groups so accounting/export flows can aggregate excise sales in the categories operators actually need.

P2:

- support planned signer rotation with overlapping saved bindings and explicit `active_from` / `active_to` windows;
- cache the last successful PRRO discovery / `infoRro` snapshot with timestamp and show it in operator UX alongside live probe results;
- show whether the current signer is eligible only as cashier or also as senior cashier according to DPS metadata.
- add local operational goods reports (daily/monthly) for accountants/operators, with aggregation by PRRO, product, fiscal-policy group, and declaration-oriented excise subgroup;
- keep those reports explicitly separate from fiscal X/Z reporting so accounting exports do not get confused with fiscal-server documents.

P3:

- warn operators before certificate expiry (for example 30/14/7-day windows and an explicit "expires soon" state);
- write an audit trail for key inspection, trust-material pull, PRRO discovery, bind changes, and signer-window changes;
- keep manual fiscal-number entry as an escape hatch only when discovery is unavailable, and mark it as degraded/manual mode.

## 10. Acceptance Test Matrix

Minimum acceptance tests before pilot:

- open shift online;
- sell online;
- return online;
- service in/out online;
- cash withdrawal if enabled by backend profile;
- close shift online;
- Z-report/fiscal daily report;
- idempotent replay of sale;
- channel switch forbidden during active shift;
- duplicate excise mark rejected;
- offline without range rejected;
- offline range allocation uses one number once;
- offline continuous limit 36h enforced;
- offline monthly limit 168h enforced;
- offline local receipt does not equal DPS ACK;
- offline sync sends documents in order;
- DPS Unified Window channel can submit/poll mocked fiscal documents;
- DPS PRRO fiscal server channel can submit/poll mocked fiscal documents;
- reconciliation preserves original submission channel;
- cross-channel retry during an open shift is rejected;
- **online** Z-report blocked while legally required offline sync is pending; **offline-mode** Z_REPORT local close-of-day is the explicit allowed Pattern C exit per M3b W10 (`docs/OFFLINE_SHIFT_CLOSE_DECISION.md` §0) and is NOT covered by this blocker;
- recovery after crash during prepared/signed/sent/offline states;
- crypto timeout moves document to retryable/manual path;
- transport timeout moves document to retryable/manual path;
- startup readiness waits for recovery;
- production config rejects passthrough crypto;
- admin retry works only for manual reconciliation state.

## 11. Operational Documentation To Maintain

Required docs:

- `docs/INSTALL.md`: installation and local run;
- `docs/OPERATIONS.md`: health, startup, backup, recovery, monitoring;
- `docs/UPGRADE.md`: upgrade/rollback and migration policy;
- `docs/LEGAL_INVARIANTS.md`: current legal engineering constraints;
- `docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md`: requirement-to-test mapping;
- `docs/PROTOCOL_SHAPE_AUDIT.md`: Maria/WebCheck session behavior;
- `docs/DPS_TRANSPORT.md`: DPS integration profile and limitations;
- `docs/OFFLINE_SYNC.md`: offline state machine, sync, errors, recovery;
- `docs/ARCHIVE_POLICY.md`: archive layout, retention, integrity checks.

## 12. Definition Of Done

For every sprint:

- code changes are minimal and scoped to sprint goals;
- migrations are idempotent and checksum-safe;
- state transitions are explicit and tested;
- tests include happy path and failure/recovery path;
- docs are updated with behavior and operational impact;
- no production path uses test credentials, mock endpoints, or passthrough signing;
- audit/trace visibility exists for every fiscal state transition;
- old user/operator workflows are not silently broken.

## 13. Current Risk Register

High risks:

- `DPS_UNIFIED_WINDOW` transport not implemented — second DPS contour missing;
- `GO_OFFLINE` triggered by manual DB state rather than API flow — offline E2E not fully automated;
- crypto sidecar is single-threaded PoC without TLS or auth — not production-safe;
- rate limiting absent — ingress unthrottled under abusive local clients.

Medium risks:

- cash balance not carried over between shifts in DPS XML — fiscal rounding exposure;
- excise goods E2E pipeline (adapter → write-path → XML) not fully tested;
- `ASK_OFFLINE_CODES` API-flow untested — offline range request may surface edge cases;
- Maria/WebCheck may require session aggregation beyond simple command mapping;
- channel lock must remain enforced in worker transaction, not only ingress.

Low risks:

- `signed_payload` type annotation drift (`str` vs `bytes`) in DPS path;
- pytest markers absent — unit/integration/e2e not distinguished in CI;
- four required operational docs still missing (`PROTOCOL_SHAPE_AUDIT.md`, `DPS_TRANSPORT.md`, `OFFLINE_SYNC.md`, `ARCHIVE_POLICY.md`).

## 14. Recommended Immediate Next Step

**Sprint 11: Offline Full Lifecycle** — closes the highest-severity remaining block.
See `docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md` §10 for full sprint breakdown (Sprint 11–14).

Do not implement DPS transport before correcting offline state semantics. The legal risk of confusing local offline receipt creation with fiscal server acknowledgement is more important than adding another transport.
