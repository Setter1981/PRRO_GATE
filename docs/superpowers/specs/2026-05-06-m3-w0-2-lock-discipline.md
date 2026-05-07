# M3 W0-2 findings — lock discipline

**Status:** research findings, not yet ratified.  Closes nothing —
PRRO_GATE-k99 remains open until M3a implementation lands the
chosen contracts in code.

**Inputs:**
- `docs/M2-handoff.md` (§4.1 invariant #1; §1 W2 cert_refresher precedent)
- `docs/superpowers/plans/2026-05-06-m3-w0-research.md` (Task 2 acceptance)
- `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md` (§3 happy-path sequence diagram, landed at 18e2247)
- `CLAUDE.md` (frozen invariants)
- `rust/prro/src/db/tx.rs` — `with_immediate` helper
- `rust/prro/src/db/repositories/fiscal_documents.rs` — `allowed_transition` + `transition_state`
- `rust/prro/src/db/repositories/ingress_inbox.rs` — current canonical use-site
- `rust/prro/src/services/cert_refresher.rs` — W2 boundary precedent
- `src/prro_gateway/services/write_path.py` — Python boundary reference
- `src/prro_gateway/services/ingress.py`
- `src/prro_gateway/services/reconciliation.py`
- `src/prro_gateway/services/offline_sync.py`
- `src/prro_gateway/services/cert_provisioning.py`
- `src/prro_gateway/services/cert_watch.py`
- `src/prro_gateway/services/backup.py`
- `src/prro_gateway/services/retention.py`
- `src/prro_gateway/runtime/container.py`
- `src/prro_gateway/runtime/rest_app.py`
- `src/prro_gateway/runtime/supervisor.py`
- `src/prro_gateway/migrations/runner.py`
- `src/prro_gateway/admin_ui/routes.py`

**Out of scope:** state-machine enumeration (W0-1, landed at 18e2247),
retry/recovery (W0-3), M3a implementation, ADR commits, offline lifecycle (M3b).

---

## 1. Python `with_immediate` / `BEGIN IMMEDIATE` audit

Python has no `with_immediate` helper; every site is a literal
`conn.execute('BEGIN IMMEDIATE')` paired with `conn.commit()` /
`conn.rollback()`.  This audit enumerates every grep-discovered
site (excluding compiled `.pyc`).  The verdict columns ("Crypto
inside?", "Network inside?") are the test against M2 §4.1
invariant #1.  "M3-relevant" lifts identify what the Rust port
must replicate or improve.

### 1.1 Hot-path sites (write_path / ingress / reconciliation / offline_sync)

These run in the synchronous fiscal pipeline and are the spine
of invariant #1 enforcement.

| # | File:line | What FK rows it protects | Legitimately inside | Crypto inside? | Network inside? | M3-relevant lift |
|---|-----------|--------------------------|---------------------|----------------|-----------------|------------------|
| 1 | `services/ingress.py:111` | `ingress_inbox` (insert / replay-detect) | `InboxRepository.accept_command` (probe + insert) | no | no | mirrors `rust/prro/src/db/repositories/ingress_inbox.rs:67` already; same shape — clean |
| 2 | `services/write_path.py:195` (`_stage_acquire_and_validate`) | `ingress_inbox` lease + `node_state` reads + `fiscal_documents INSERT` + `node_state.next_lnd UPDATE` + `audit_log` | lease acquire, mode/breaker fast-path, guards, lnd allocation, doc INSERT | no | no | **canonical Stage-1 lock** — W0-1 §3.1 inherits this shape. No Python drift |
| 3 | `services/write_path.py:244` | `audit_log` only (CRYPTO_BREAKER_BLOCKED audit emit) | single `AuditRepository.log_event` | no | no | minor: re-opens BEGIN IMMEDIATE inside the same logical request after `_mark_error_locked` already committed; effectively a "second short tx" pattern |
| 4 | `services/write_path.py:539` (`_handle_management_command_locked` mode flip) | `node_state.mode` UPDATE + `audit_log` | mode flip + audit for management ops | no | no | clean |
| 5 | `services/write_path.py:577` / 600 (mark_error_locked branches) | `fiscal_documents.state` UPDATE + `ingress_inbox.status` + `audit_log` | error-finalize bookkeeping | no | no | mirrors §1.4 `transition_state` requirement (call inside lock) |
| 6 | `services/write_path.py:737` (`_stage_sign` post-sign persist) | `fiscal_documents.state PREPARED→SIGNED` + `document_files(SIGNED_XML, PAYLOAD_XML)` + `audit_log` + optional `node_state.mode` flip | persist signed bytes that were computed BEFORE this BEGIN | no | no — `crypto_provider.sign_*` happens at write_path.py:~660 BEFORE this line | **gold standard for M3a stage-3 sign**: sign bytes hoisted above the lock; persist-only inside.  Mirrors W2 cert_refresher pattern |
| 7 | `services/write_path.py:790` (`_stage_send_or_offline` pre-send) | `fiscal_documents.state →SENDING` + (later) `transport_trace` | mark SENDING before wire — invariant 1 |  no | no | **gold standard for M3a stage-4-pre (Pattern B per §5.2 + ADR-M3-A5)**: M3a Rust MUST mirror this — CAS Signed→Sending inside `with_immediate`, commit, release, then call `DpsChannel::send_chk` outside the lock.  Earlier W0-2 drafts considered collapsing SENDING into the post-send lock; that recommendation was withdrawn after senior review (DPS does not deduplicate per `:148`; the SENDING marker is the only crash-resume safety mechanism — even a single-process binary CAN crash mid-wire) |
| 8 | `services/write_path.py:874` (post-send commit) | `fiscal_documents.state SENDING→SENT` + `transport_trace` | persist server response + state flip | no | no — `transport_client.send` runs at :806-820 OUTSIDE this lock | **gold standard for M3a stage-4 send-persist**: wire I/O outside; persist-only inside |
| 9 | `services/write_path.py:921` (MAC recovery first commit) | `fiscal_documents.state` rollback for MAC recovery | state-rollback + audit | no | no | M3b territory (MAC recovery is offline-adjacent) |
| 10 | `services/write_path.py:1003` (MAC recovery re-persist after second sign) | `document_files DELETE + INSERT` of corrected PAYLOAD_XML/SIGNED_XML | replace stale artifact bytes | no | no — sign happens BEFORE :1003 (see :1001 hand-back of `signed_payload`) | **secondary sign-persist precedent**: confirms the "compute-outside, persist-inside" pattern is already applied even on the recovery path |
| 11 | `services/write_path.py:1053` (`_stage_finalize_ack`) | `fiscal_documents.state →ACK` (or OFFLINE_LOCAL_ACK), `excise.mark_*`, `outbox.enqueue_document`, `audit_log`, shift side-effects | terminal-success bookkeeping | no | no | **gold standard for M3a stage-5 finalize**: nothing transactional touches the wire |
| 12 | `services/write_path.py:1361` (post-MAC-recovery success) | `audit_log` MAC_RECOVERY_SUCCESS | single audit | no | no | clean |
| 13 | `services/reconciliation.py:83` | `audit_log` (RECONCILE_COOLDOWN_SKIP) | single-row audit | no | no | clean |
| 14 | `services/reconciliation.py:112` | `fiscal_documents.state →ACK` + `outbox.enqueue` + `audit_log` + `transport_trace` | ACK finalize from poll | no | no — `transport_client.fetch_status` runs in `_run_poll_phase` BEFORE this method | clean: poll happens in phase 2 worker pool BEFORE `_apply_poll_result` enters the lock |
| 15 | `services/reconciliation.py:164` | `fiscal_documents.state →REJECTED` + audit + trace | REJECTED finalize from poll | no | no | clean |
| 16 | `services/reconciliation.py:207` | `fiscal_documents.state →ERROR_RETRYABLE/REQUIRES_MANUAL` + recovery counter | retry / escalate finalize from poll | no | no | clean |
| 17 | `services/offline_sync.py:119` | `document_files DELETE + INSERT` of corrected PAYLOAD_XML/SIGNED_XML for offline-deferred sign | persist already-signed bytes | no | no — `sign_raw` at :108 runs BEFORE :119 | **PRECEDENT**: same hoist pattern Python uses for offline deferred sign |
| 18 | `services/offline_sync.py:218` | `fiscal_documents.state` (OFFLINE_LOCAL_ACK→ACK/SENT/REJECTED/ERROR_RETRYABLE) + outbox + audit + trace + `apply_shift_side_effects_locked` | per-doc finalize after batch send | no | no — `transport_client.send` happens at :177-190 BEFORE :218 | **gold standard for M3b offline drain**: wire I/O occurs OUTSIDE the per-doc finalize lock |
| 19 | `services/offline_sync.py:379` | `node_state.mode →ONLINE`, `offline_session.status →CLOSED`, `audit_log` | online-recovery transition after drain | no | no | M3b territory |

### 1.2 Cert lifecycle / runtime / boot sites

These run outside the receipt hot path but matter for invariant
parity.

| # | File:line | What FK rows it protects | Legitimately inside | Crypto inside? | Network inside? | M3-relevant lift |
|---|-----------|--------------------------|---------------------|----------------|-----------------|------------------|
| 20 | `services/cert_provisioning.py:555` | `operator_certs` upsert + `audit_log` | persist freshly-provisioned cert | no — provisioning fetch happens in caller before this method | no | mirrors Rust W2 cert_refresher precedent (`compute_fingerprint` hoist) |
| 21 | `services/cert_watch.py:409` / 473 | cert-watch audit + cache table | watch-trigger audit / cache update | no | no | clean |
| 22 | `services/backup.py:83` / 127 | `backup_metadata` row + `audit_log` | backup begin/end metadata commit | no | no — `sqlite3.backup()` opens its OWN BEGIN IMMEDIATE per-page; `services/backup.py:6,54` doc-comment explicitly explains the interleave | **noteworthy**: this is the only site where ANOTHER BEGIN IMMEDIATE (the backup's per-page lock) interleaves; documented and benign |
| 23 | `services/retention.py:62` | retention sweep deletes (TTLed `audit_log`, etc.) | bulk DELETE | no | no | clean (M3 unaffected) |
| 24 | `migrations/runner.py:81` | `_schema_migrations` checksum + DDL apply | migration-runner write | no | no | not on hot path; still DB-only |
| 25 | `runtime/container.py:321` / 391 / 484 / 776 / 872 | startup seed: FN config, node_state, integrity-check flips | App::boot writes | no | no | **directly relevant to W0-2 §6** — App::boot lock-discipline |
| 26 | `runtime/rest_app.py:503` / 572 | request-scoped audit_log + lifecycle table | per-request lifecycle commit | no | no | analogous Rust path will be axum/tonic handler — same invariant |
| 27 | `runtime/supervisor.py:96` | supervisor heartbeat row | lifecycle heartbeat | no | no | clean |
| 28 | `admin_ui/routes.py:787` | admin form-driven config UPDATE + audit | UI-driven write | no | no | clean |

### 1.3 Aggregate verdict

**Invariant #1 is currently respected by Python at every audited
site.**  Concretely:

- **Crypto** is hoisted above the lock at every site (sign at
  `write_path.py:~660` precedes `:737`; `sign_raw` at
  `offline_sync.py:108` precedes `:119`; provisioning fetch
  precedes `cert_provisioning.py:555`).
- **Network** is hoisted above the lock at every site
  (`transport_client.send` at `write_path.py:806-820` precedes
  `:874`; `fetch_status` runs in the reconciliation phase 2 pool
  before `_apply_poll_result` enters its lock at `:112/164/207`;
  `transport_client.send` at `offline_sync.py:177-190` precedes
  `:218`; CMP fetch in cert-provisioning runs before
  `cert_provisioning.py:555`).

**No drift discovered.** The Python codebase is the **behavioural
oracle** that M3 Rust is being held to: the lift-out for M3a is
to preserve, not repair, the discipline.

**One caveat — interleaving.** The intra-method "second
BEGIN IMMEDIATE" pattern (e.g. `write_path.py:244` re-opens after
`_mark_error_locked` commits) is a Python-side stylistic
artefact.  Rust must be careful that the equivalent flow in
async/await idiom does not accidentally hold the first lock
across the second `with_immediate` (which would be a SQLite-level
error: nested BEGIN IMMEDIATE).  The Rust `with_immediate` helper
already returns `Result<R>` and acquires + releases its own
connection from the pool, so this is structurally avoided —
documented for completeness in §3.

**Aggregate count:** 36 distinct `conn.execute('BEGIN IMMEDIATE')` call sites
(verified `grep -rnE "conn\.execute\(['\"]BEGIN IMMEDIATE['\"]" src/prro_gateway/`).
Per-file breakdown: `write_path.py` 12, `runtime/container.py` 5,
`reconciliation.py` 4, `offline_sync.py` 3, `cert_watch.py` 2,
`backup.py` 2, `runtime/rest_app.py` 2, plus 1 each in
`retention.py`, `ingress.py`, `cert_provisioning.py`,
`runtime/supervisor.py`, `migrations/runner.py`, `admin_ui/routes.py`.
All 36 sites verified clean — no network/crypto/foreign-IO inside any
`BEGIN IMMEDIATE` block. No unresolved markers surfaced from this audit.

---

## 2. M3a transactional-boundary table per pipeline stage

This section is the **spine of the spec**.  Each row maps the
W0-1 §3.1–§3.5 pipeline stage onto an explicit lock contract.
"CAS-only" means a single SQL statement with a `WHERE state = ?`
clause that under SQLite WAL auto-promotes to a brief implicit
write tx.  "no lock" means a connection acquisition with no
write at all (read-only).  "with_immediate" means the
`rust/prro/src/db/tx.rs:11` helper.

### 2.1 Stage table

| Stage | Lock kind | Protects (FK rows / state transitions) | Hoisted ABOVE the lock | Deferred AFTER lock release | Failure mode if violated |
|-------|-----------|----------------------------------------|------------------------|------------------------------|--------------------------|
| **0. Inbox accept** (pre-stage, runs in ingress shell) | `with_immediate` | `ingress_inbox` row insert; replay-detect (probe + insert atomic on `(fn, idem_key)`) | canonical-payload SHA-256 fingerprint (computed in adapter); JSON canonicalisation | downstream worker dispatch (the per-FN write_path worker is notified out-of-tx) | Idempotency violation — same `(fn, idem_key)` could yield two inserts; CAS race silent |
| **1. acquire+validate** (W0-1 §3.1) | `with_immediate` | `ingress_inbox.status NEW→PROCESSING` (lease); `node_state` reads (snapshot under tx); `node_state.next_lnd UPDATE…RETURNING`; `INSERT fiscal_documents(state=PREPARED)`; `audit_log` append | nothing (this stage is the entry; pre-tx work is just lease-token generation) | nothing (this stage is pure DB; no I/O at all) | lnd burnt across crash; lease + doc-INSERT split could leak rows; UNIQUE(fn,lnd) constraint protects against drift but BEGIN IMMEDIATE is what serialises concurrent writers per-FN  |
| **2. guard** (W0-1 §3.2 — sub-stage of 1 in Python) | (folded into stage 1 lock) | n/a — same lock as stage 1 | n/a | n/a | n/a |
| **3a. sign — compute** (W0-1 §3.3, before lock) | **no lock** | n/a (no DB writes, only reads to load FN config / active cert) | `prro::xml::build_canonical_xml(&CanonicalDoc)`; resolve prevhash MAC (read-only against ACKed previous doc's PAYLOAD_XML SHA — immutable post-ACK); `CryptoProvider::sign_cms_detached(SignCmsRequest)` (M2 frozen; may use `tokio::task::spawn_blocking`) | n/a | invariant #1 violation if any of these run inside a `with_immediate` — long async wait inside tx blocks all other writers per-pool, deadlocks, holds RESERVED lock past the wire |
| **3b. sign — persist** (W0-1 §3.3, lock) | `with_immediate` | `transition_state(Prepared, Signed)` CAS; `document_files INSERT(SIGNED_XML)`; optional `document_files INSERT(PAYLOAD_XML)` (DPS profile only); optional `node_state.mode CRYPTO_DEGRADED→ONLINE` flip on hysteresis success; `audit_log` append | `signed_payload: SignedCmsBytes` (or detached XMLDsig bytes), pre-computed `signed_xml_sha256`, `payload_xml_sha256`, fingerprints (any digests of the bytes that need persistence — compute outside, pass by value into the closure) | nothing — sign-persist is purely DB; the next step (send) is its own NO-LOCK stage | lock holdtime balloons if CMS computed inside; deadlock with concurrent FN; W2 cert_refresher precedent (`cert_refresher.rs:291`: `compute_fingerprint` hoisted) |
| **4-pre. send — mark intent** (Pattern B per ADR-M3-A5; new in M3a) | `with_immediate` | `transition_state(Signed, Sending)` CAS (or `Encrypted, Sending` for Checkbox-flow); `submission_attempted_at` UPDATE; `audit_log` append `STAGE_SEND_INTENT_MARKED` | nothing (pre-tx work is connection acquire only) | nothing — the lock releases BEFORE the wire I/O begins (4a) | invariant #1 violation if wire I/O leaks into this lock; AND structural duplicate-send hazard if this stage is skipped (recovery cannot distinguish "stage 4 hasn't started" from "stage 4 sent but reply lost") |
| **4a. send — wire I/O** (W0-1 §3.4, between locks) | **no lock** | n/a (read-only loads of `transport_profile` happen earlier; DPS channel is shared) | nothing (Pattern B requires the Sending CAS to commit BEFORE wire send; the wire payload `signed_payload: SignedCmsBytes` was already hoisted above 3b) | n/a | invariant #1 violation if RPC awaited inside lock — RESERVED lock held over network round-trip; entire writer pool starves.  ALSO: wire send MUST NOT precede the `Sending` CAS commit, or the crash-resume contract (W0-3 §3 SENDING row) is violated |
| **4b. send — persist outcome** (W0-1 §3.4, lock) | `with_immediate` | `transition_state(Sending, Sent)` CAS on wire OK (or `Sending, Kvt1` if endpoint returned KVT1 inline + `document_files INSERT(KVT1_RAW)`; or `Sending, ErrorRetryable` on wire failure with known state per W0-3 §2 mapping); `transport_request_id` set; `transport_trace` INSERT; `audit_log` append | wire response (`SendChkResponse` DTO + `kvt1_raw: Option<Vec<u8>>` if returned) | KVT1→KVT2 reconciliation poll (W0-3 owns the trigger; for M3a the inline KVT2 from a single RPC is permitted, otherwise polling deferred) | if wire response merged into the lock-open call inadvertently, send retry logic doubles the RPC; idempotency hole at the wire (Pattern B's whole reason to exist) |
| **5. finalize** (W0-1 §3.5) | `with_immediate` | `transition_state(Kvt2, Ack)` CAS; `node_state.last_known_unsigned_xml_sha256` UPDATE (next-doc MAC chain seed); `ingress_inbox.status →DONE`; `audit_log` append | `unsigned_xml_sha256` (already persisted in `fiscal_documents` since stage 1; just read-back) | outbox publish to CLOUD_HUB / receipt rendering / metric emission — these run AFTER the lock releases (W0-1 §3.5 lists outbox INSERT inside the lock; M3a may keep it inside since it is a pure DB write — see **finalize note** below) | next-doc MAC chain seed leaks if `last_known_unsigned_xml_sha256` not committed atomically with the ACK transition |

### 2.2 Reviewer-spotcheck cells

The reviewer is expected to sample 3-5 cells against W0-1 §3
sequence diagram and Python source.  Concrete map:

- **Stage 1 "Hoisted ABOVE"** = Python equivalent
  `write_path.py:194` (BEGIN IMMEDIATE is the **first** statement
  of `_stage_acquire_and_validate`; nothing precedes it).  Rust
  may pre-build the lease-token string before the `with_immediate`
  call (the Python `lease_token` is built at
  `write_path.py:130-131` BEFORE `_stage_acquire_and_validate` is
  called — this is fine; lease-token generation is pure CPU).
- **Stage 3a "Hoisted ABOVE"** = Python equivalent
  `write_path.py:~660-735` calls the crypto provider; line 737
  is where BEGIN IMMEDIATE then opens.  Mirror W2 cert_refresher
  `cert_refresher.rs:291` `compute_fingerprint` precedes
  `cert_refresher.rs:292` `with_immediate`.
- **Stage 4-pre "with_immediate"** (Pattern B; ADR-M3-A5) =
  Python equivalent `write_path.py:790` — `BEGIN IMMEDIATE`
  with `update_state(state=DocumentState.SENDING,
  expected_states=(PREPARED, SIGNED, ENCRYPTED))` followed by
  `conn.commit()` at `:803`; the wire send call at `:806-820`
  runs strictly AFTER this commit.
- **Stage 4a "no lock"** = Python equivalent
  `write_path.py:806-820` — `transport_client.send` runs
  OUTSIDE the lock, AFTER the SENDING-marker commit; the
  post-send `with_immediate` opens at `:874`.
- **Stage 5 "Deferred AFTER"** = Python `_apply_shift_side_effects_locked`
  at `write_path.py:1069` runs INSIDE the lock; outbox enqueue
  also inside.  Documented as a deliberate choice: those are
  pure-DB writes and belong inside the same atomic finalize.
  Outbox **publish** (the cross-process notification, if any) is
  the post-commit step.

### 2.3 Finalize note

There is a tension at the finalize stage: the W0-1 sequence
diagram lists outbox INSERT, last-known-MAC update, and
inbox.status=DONE all inside one `with_immediate`.  This is
correct because they are **all DB writes** and finalize must be
atomic against the next document's MAC chain seed read.  The
Python reference does the same.  **Outbox publish (the network
fan-out) is OUTSIDE this lock** and is separate from outbox
INSERT (the persistence).  M3a impl must not conflate them.

### 2.4 Stage 3b/4-pre/4b idempotency note

The "persist after I/O" stages MUST be idempotent against
double-entry.  Crash-resume scenarios per Pattern B + ADR-M3-A5
(W0-2 §5.2):

- **Crash AT 3b commit**: doc remains in SIGNED (sign succeeded
  but the persist-row update did not).  Recovery re-drives
  forward via stage 4-pre + stage 4 — the re-sign of the same
  CMS bytes is referentially transparent (M2 W4 byte-equiv
  contract) and the re-acquire of `with_immediate` lands the
  CAS that was missed.
- **Crash AT 4-pre commit**: doc is now in SIGNED (CAS to
  SENDING did not commit) — wire send has NOT happened yet
  (Pattern B requires SENDING commit before send_chk).  Safe
  to re-drive via stage 4-pre.
- **Crash AT 4a (between 4-pre commit and 4b commit)**: doc is
  in SENDING.  This is the dangerous case Pattern B exists to
  trap.  Recovery rule per W0-3 §3 SENDING row: CAS
  Sending→ErrorRetryable + audit
  `crash_resume_sending_to_error_retryable`; do NOT auto-re-send
  (DPS does not deduplicate per `write_path.py:148`).  Operator
  resolves via manual `last_chk` consultation.
- **Crash AT 4b commit**: doc remains in SENDING (wire reply
  was received but persist-row update did not commit).  Same
  rule as crash-at-4a — recovery cannot tell the cases apart
  and routes to ErrorRetryable for operator inspection.

W0-1 §2.1 + ADR-M3-A5 transition matrix: every CAS at 3b /
4-pre / 4b uses `expected_states` (the source-state argument)
to narrow idempotent recovery.  Pattern B's SENDING marker
turns "did the wire fire?" from a runtime guess into a
persisted state machine signal — that turns idempotency from
"review-only" into a per-CAS structural guarantee.

---

## 3. `db::tx::with_immediate` Rust helper contract

**Authoritative source.** `rust/prro/src/db/tx.rs:11-36`:

```rust
pub async fn with_immediate<R, F>(pool: &SqlitePool, f: F) -> anyhow::Result<R>
where
    F: for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, anyhow::Result<R>> + Send,
    R: Send,
```

Body: `pool.acquire()` → `BEGIN IMMEDIATE` → run `f(conn)` →
`COMMIT` on Ok / `ROLLBACK` on Err / best-effort `ROLLBACK` if
COMMIT itself fails (lines 19-34).  The closure receives a
`&mut SqliteConnection`, NOT a `Pool` and NOT a `Transaction<_>`
struct — meaning the helper holds the lock-state implicitly via
the `BEGIN/COMMIT` SQL bracketing rather than via a typestate
RAII guard.

### 3.1 What we want the helper to forbid

The user-asserted goal: **no foreign IO `await` inside the
closure** — meaning no network call, no crypto provider call, no
file-system I/O outside the FK rows being protected, no DPS, no
CMP, no sidecar.  This is the operationalisation of M2 §4.1
invariant #1.

Concretely, "foreign IO" inside a `BoxFuture<'c, _>` means:
1. `.await` on a `Future` whose poll involves a non-DB syscall
   (`tokio::net::*`, `reqwest::*`, `tonic::*`, `tokio::fs::*`,
   `tokio::process::*`).
2. `.await` on a `JoinHandle` returned from
   `tokio::task::spawn_blocking` whose closure does crypto work
   (`prro_crypto::*` `spawn_blocking` wrappers,
   `CryptoProvider::sign_*`).
3. `.await` on a `Future` returned from a method we deliberately
   don't want called inside a tx (e.g. `CryptoProvider::*`,
   `DpsChannel::*`).

Each option below is evaluated against this concrete enemy list.

### 3.2 Option (a): compile-time enforcement via wrapper type / closure type bounds

**Idea.** Make the closure parameter type carry a marker that
forbids specific traits.  E.g. require the future to implement
a custom `TxSafe` marker auto-trait that is NOT implemented for
`reqwest::Response`, `tonic::Response<T>`, etc.

**Feasibility verdict for the M3 enemies:** **NOT FEASIBLE FOR
GENERAL CASE — POLICY ONLY for arbitrary async fn calls inside
the closure**.

Why: Rust's auto-trait inference sees the closure body's future
as an opaque `impl Future`.  An auto-trait like `Send`/`Sync`
propagates through every captured value, but we cannot define a
**negative** auto-trait that "is implemented for futures EXCEPT
those that touch tokio::net".  Negative reasoning over auto
traits is not part of the Rust type system today.

The narrow case where compile-time enforcement IS feasible:
**`Send + Sync + 'static` on captured values** — already enforced
by the existing `F: …+ Send` bound at `tx.rs:13`.  This catches
`!Send` types like `Rc<…>`, `RefCell<…>` — but a `tonic::Channel`
IS `Send + Sync`, so it sails through.

**Sub-option (a1): typestate trick — closure receives a
`SqliteConnection` AND a phantom token; the token's `Drop` impl
panics if certain methods were called.**  Rust does not give us
the introspection to check method calls at compile time;
phantom tokens can only check that the closure consumed/dropped
them in a particular order, not that crypto was avoided.
**REJECTED — does not enforce the actual invariant**.

**Sub-option (a2): wrap the connection in a newtype
`TxConnection<'c>` whose impl forbids the methods we don't want
called.**  We can't impose this on third-party crates: the
crypto provider doesn't take a `TxConnection`; it takes nothing
DB-related at all (W5 static check enforces no `SqlitePool` /
`SqliteConnection` in `crypto::` / `transports::` public API).
So forbidding methods on `TxConnection` doesn't help — the bad
calls don't go through it.

**Verdict — option (a):** `ENFORCEABLE AT COMPILE` for the
narrow `Send` bound only (already in force at `tx.rs:13`); for
the actual M3 enemy list (foreign network IO, crypto IO inside
the closure) — **POLICY ONLY — not enforceable at compile time**.

### 3.3 Option (b): runtime `debug_assert!`

**Idea.** Inside the closure, observe a misuse signal and panic.
Possible signals:
- A thread-local "in-tx" flag that crypto/dps modules check
  before doing IO and panic if set.
- A counter in the `with_immediate` wrapper that increments
  before `f.await` and decrements after, and that
  `CryptoProvider`/`DpsChannel` impls assert is zero on entry.

**Feasibility verdict — depends on the misuse class:**

- For `tokio::task::spawn_blocking` paths
  (`InProcessProvider::sign_cms_detached`,
  `InProcessProvider::unwrap_envelope`,
  `InProcessProvider::fetch_cert_by_ski` per M2-handoff §1 W1)
  the JoinHandle's `.await` is observable from a wrapper if the
  wrapper sets the thread-local before `.await`-ing the
  JoinHandle.  We can panic before the spawn_blocking dispatch
  if the in-tx flag is set.  **ENFORCEABLE AT RUNTIME** for this
  class via a `debug_assert!(!IN_TX.load())` at the top of every
  `InProcessProvider::*` method.
- For `tonic::Request<T>::send_*` paths (`DpsChannel`) —
  similarly, the entry point of every `DpsChannel` method can
  `debug_assert!`.  **ENFORCEABLE AT RUNTIME**.
- For arbitrary `async fn foo()` written by a future engineer
  that does network IO — the Future is opaque from the
  `with_immediate` wrapper.  No `.await` site can observe it
  was inside a tx unless that fn ALSO opts into the
  `debug_assert!`.  **NOT ENFORCEABLE AT RUNTIME** generally.

**The user's controller-asserted constraint is correct:** a
runtime `debug_assert!` in `with_immediate` itself cannot
universally catch "foreign IO `.await` inside the tx closure"
because the future returned by an arbitrary `async fn` is
opaque to the wrapper.

**Verdict — option (b):** `ENFORCEABLE AT RUNTIME` for the
specific known-evil callees (`CryptoProvider`, `DpsChannel`)
via a thread-local guard set by `with_immediate` and asserted at
every M2 substrate entry point.  **POLICY ONLY** for arbitrary
future async fns.

### 3.4 Option (c): convention + W5-style static check

**Idea.** Extend the existing `rust/prro/tests/api_surface_no_db_handle.rs`
syn-based AST scanner (M2-handoff §1 W5) with a new lint:
"inside a `with_immediate(pool, |conn| Box::pin(async move { … }))`
closure body, no `.await` may target a path containing
`crypto::`, `CryptoProvider::`, `DpsChannel::`, `transports::dps::`,
`reqwest::`, `tonic::`, `tokio::net::`, `tokio::fs::`."

**Feasibility verdict:** **ENFORCEABLE BY STATIC SCAN** for
named callees that match the path-prefix list.  W5 has already
proved the syn-based approach works for the
"`SqlitePool`/`SqliteConnection`/`Pool`/`Transaction` not in
public API" rule, including obfuscation paths
(`impl Trait`/`dyn Trait`/`Box<dyn FnMut(SqlitePool)>`).  The
scanner can be extended to walk function bodies, find calls
nested inside `with_immediate(_, |_| Box::pin(async move { … }))`,
and assert the call-target path does not match the forbidden
list.

**Limitations:**
1. Method calls through trait objects whose path is hidden
   (e.g. a captured `Arc<dyn CryptoProvider>` whose name is
   reassigned to a local `let p = self.crypto.clone();`) still
   appear in the AST as `p.sign_cms_detached(...)` — the
   scanner needs a name-resolution step OR a per-method-name
   denylist.  W5's name-resolution is fixture-based — the new
   lint would do similarly: deny by **method name**
   (`sign_cms_detached`, `verify_dstu`, `unwrap_envelope`,
   `fetch_cert_by_ski`, `send_chk`, `last_chk`, `ping`,
   `status_rro`, `info_rro`, `query_by_local_identity`,
   `by_server_fiscal_no`).  Still leaves room for renamed-impls
   to slip through, but raises the bar to "deliberately
   bypass".
2. Indirect dispatch through helper-function-of-helper-function
   chains is not catchable unless the scanner walks the
   call-graph — out of W5 scope.  **POLICY** for that depth.

**Verdict — option (c):** `ENFORCEABLE BY STATIC SCAN` for the
finite, bounded list of M2 substrate API method names.
**POLICY ONLY — not enforceable** for adversarial / refactored
call chains that go through helper indirections.

### 3.5 Option (d): hybrid

**Idea.** Combine (b) + (c).  Static scan catches direct
callsites; runtime `debug_assert!` in `CryptoProvider` /
`DpsChannel` entry points catches indirection that the scanner
missed (e.g. `let p = self.crypto.clone(); inner_helper(p)`
where `inner_helper` then calls `p.sign_cms_detached`).

**Feasibility verdict:** This is the **MOST ROBUST**
configuration.

- Compile-time: existing `F: …+ Send` bound on `tx.rs:13`
  catches `!Send` captures (already in force).
- Static scan: catches the obvious calls in the closure body —
  most pull-requests will fail at this gate.
- Runtime: catches the indirect case — `debug_assert!` in
  CI/test runs panics; production runs in `release` mode skip
  the assert (so no perf impact).

**Picked? See decision below.**

### 3.6 Decision — primary + fallback

**Primary: option (d) — hybrid.**

M3a MUST implement:

1. **Keep** the `F: ... + Send` bound on the closure type — it
   is necessary (catches `!Send` captures) but not sufficient.
   **Change** the closure-argument type per §4.4 / ADR-M3-A4
   from `&'c mut SqliteConnection` to `&'c mut WriteTxConn<'c>`
   (or one of the fallback HRTB shapes documented in
   ADR-M3-A4 if the borrow checker rejects this exact shape).
   The `with_immediate` body constructs `WriteTxConn` via the
   module-private `WriteTxConn::new(&mut *conn)` and passes
   `&mut wt` into the closure — see §4.2 sketch.

2. **Extend** `rust/prro/tests/api_surface_no_db_handle.rs`
   (or create a new sibling test
   `rust/prro/tests/with_immediate_no_foreign_io.rs`) that does
   AST scanning of the closure body of every
   `with_immediate(...)` call site in `rust/prro/src/`, denying
   `.await` paths that match the M2 substrate method-name
   denylist.  This is the W0-2 W5-extension lint.  Effort: ~1
   day; reuse W5's syn 2 + quote 1 deps.

3. **Add** a `tokio::task_local!` `IN_WITH_IMMEDIATE` in
   `rust/prro/src/db/tx.rs` that `with_immediate` enters via
   `IN_WITH_IMMEDIATE.scope((), async { f(&mut wt).await })` so
   the marker correctly follows the future across `.await`
   boundaries on the multi-threaded tokio runtime.  In
   `prro::crypto::*` and `prro::transports::dps::*` the
   public-API entry points
   `debug_assert!(IN_WITH_IMMEDIATE.try_with(|_| ()).is_err(),
   "foreign IO inside with_immediate")`.  Compile out in release;
   panic loudly in debug + tests.

   **Why NOT `thread_local!`:**  `with_immediate` is `async fn`;
   the closure body crosses `.await` points.  Tokio's
   multi-threaded runtime is free to migrate the polled future
   between worker threads after a yield, so a `thread_local!`
   set on thread T1 is invisible on T2 where the future
   resumes.  `tokio::task_local!` is per-task (not per-thread)
   and remains visible across `.await` boundaries on the same
   task regardless of which worker polls it — that is the
   correct primitive **for catching foreign IO at provider
   public-API entry** (where the entry is `async fn` polled in
   the awaiting task's context).

   **What `tokio::task_local!` does NOT cover, and how the static
   scan complements it:**  `tokio::task::spawn_blocking(closure)`
   schedules `closure` on a dedicated blocking-pool thread.
   The closure body is a *synchronous* `FnOnce` — it is not
   polled as a future.  Tokio does NOT propagate the awaiting
   task's task-local table into the blocking-pool thread; the
   closure runs without async context.  Therefore:
   - Inside `InProcessProvider::sign_cms_detached`, when the
     provider internally calls `spawn_blocking(|| heavy_crypto())`,
     the runtime guard **inside the spawn_blocking closure body**
     would NOT fire because the task-local is invisible there.
     This is fine — the guard fires at the **public API entry**
     of `sign_cms_detached` itself, which is `async fn` running
     in the awaiting task's context where the task-local IS
     visible.  The dispatch into `spawn_blocking` happens after
     the entry-time `debug_assert!`, so the assert wins the race.
   - But the static scan (option (c)) is what catches a
     **direct** `tokio::task::spawn_blocking(...)` inside a
     `with_immediate(...)` closure body — that is, code that
     bypasses the M2 substrate's typed entry points and uses
     spawn_blocking ad-hoc.  The runtime guard cannot help
     because the spawn_blocking closure runs without
     task-local visibility; the static scan sees the
     `spawn_blocking` call expression in the AST and rejects it.
   - The static-scan denylist therefore includes both the M2
     substrate method names (per option (c) §3.4) AND the
     literal `spawn_blocking` / `block_in_place` call
     expressions, so the gates are non-overlapping (each catches
     what the other cannot).

   **Why NOT `AtomicBool`:**  per-task storage is not shared
   across threads in the way a global atomic is.  `tokio::task_local!`
   already provides scoping (`scope` + `try_with`); a unit value
   is sufficient — presence/absence of the scope is the signal.

4. **Document** in CLAUDE.md (or the M3 spec, not in the
   ADR — leave ADR amendment to coordinator) that arbitrary
   helper-fn-of-helper-fn chains are POLICY ONLY and reviewer-only.

**Fallback: option (c) alone.**  If runtime tooling
(`thread_local!` machinery, atomic-bool dance, or CI red-on-debug-build)
proves brittle, drop the runtime guard and rely on the static
scanner alone.  Static scan catches every direct callsite; the
indirection case is the residual.

**Explicit rejection of option (a) alone.**  Compile-time
enforcement via type system is **POLICY ONLY** for the M3 enemy
list and is not a viable primary mechanism (per §3.2).  The
existing `Send` bound at `tx.rs:13` is necessary but not
sufficient.

### 3.7 Sanity check vs M2-handoff and CLAUDE.md

- M2-handoff §4.1 invariant #1: "no network or crypto inside long
  SQLite write transactions" — option (d) operationalises this.
- W2 cert_refresher precedent (`cert_refresher.rs:291`,
  M2-handoff §1 W2): `compute_fingerprint` is hoisted ABOVE
  `with_immediate`.  Option (d) makes that pattern enforceable
  at static-scan time + runtime debug.
- W5 ADR gate (M2-handoff §1 W5, `api_surface_no_db_handle.rs`):
  the W0-2 lint is a sibling of the W5 test, not a replacement
  — they enforce different invariants.

---

## 4. `transition_state` helper contract

**Authoritative source.**
`rust/prro/src/db/repositories/fiscal_documents.rs:139-170`:

```rust
pub async fn transition_state(
    pool: &SqlitePool,
    id: DocumentId,
    from: DocState,
    to: DocState,
) -> sqlx::Result<TransitionOutcome>
```

Body: in-code `allowed_transition` whitelist short-circuit
(returns `Forbidden` without DB call); otherwise a single
`UPDATE … WHERE document_id = ? AND state = ?` CAS; on miss, a
follow-up `SELECT 1 … LIMIT 1` to disambiguate `Conflict` (row
exists, state diverged) from `NotFound` (row gone).

**The bd-issue PRRO_GATE-k99** (input cited in §1 of this spec)
documents the limitation: between the missed CAS UPDATE and the
disambiguating SELECT a concurrent delete/insert could swap the
two outcomes.  Benign in M1 (`fiscal_documents` is append-only-like
in production), but M3 must close the gap by ensuring every
`transition_state` invocation is wrapped in a `with_immediate`
envelope so the disambiguation pair is atomic.

The module doc-comment at
`rust/prro/src/db/repositories/fiscal_documents.rs:14-25` already
records the deferred-to-M3 expectation: "M3 write_path is
expected to call `transition_state` inside its own
`db::tx::with_immediate` envelope as part of a compound op
(transition + audit_log.append + node_state.update), naturally
making the disambiguation atomic."

### 4.1 Option (a): call-site requirement (POLICY ONLY)

**Idea.** Every caller MUST wrap the call in `with_immediate`;
enforce by review only.  No code change to `transition_state`.

**Pros:**
- Zero migration effort.  Fully backwards-compatible.
- The compound-op pattern (transition + audit_log + node_state)
  naturally requires `with_immediate` anyway, so the wrap is
  cheap to demand.
- Matches W2 cert_refresher style — `cert_refresher.rs:295-313`
  already uses this exact pattern (CAS UPDATE inside
  `with_immediate`).

**Cons:**
- POLICY ONLY — not enforceable at compile or runtime.
- Reviewer-only enforcement; first violation lands in production.
- bd-issue PRRO_GATE-k99 acceptance criterion explicitly
  contemplates this option ("verifies the wrap is in place at
  every call site"), but it requires manual auditing.

**Verdict:** `POLICY ONLY` — not enforceable beyond review.

### 4.2 Option (b): helper takes a tx-witness newtype (compile-time, sealed)

**Idea.** Introduce a sealed newtype `WriteTxConn<'a>` whose constructor is private to `db::tx` (only `with_immediate` builds it).  Change the signature to:

```rust
// db/tx.rs
pub struct WriteTxConn<'a> {
    inner: &'a mut SqliteConnection,
    _seal: (),  // private field — only this module can construct
}
impl<'a> WriteTxConn<'a> {
    // Constructor is **module-private** (`fn`, not `pub(crate)`) so
    // no other module in the crate — including repositories,
    // services, or tests outside `db::tx` — can fabricate one
    // without going through `with_immediate`.  This is the
    // structural seal: pub(crate) would let any in-crate caller
    // bypass `with_immediate` and construct a WriteTxConn from a
    // raw `pool.acquire()`, defeating the whole compile-time
    // enforcement.
    fn new(c: &'a mut SqliteConnection) -> Self {
        Self { inner: c, _seal: () }
    }

    // Test-only constructor lives behind cfg(test) inside this
    // module, NOT exposed pub(crate).  Keeps integration tests
    // able to exercise the helpers without breaking the seal.
    #[cfg(test)]
    pub(super) fn new_for_test(c: &'a mut SqliteConnection) -> Self {
        Self::new(c)
    }
}
impl<'a> std::ops::Deref for WriteTxConn<'a> {
    type Target = SqliteConnection;
    fn deref(&self) -> &Self::Target { self.inner }
}
impl<'a> std::ops::DerefMut for WriteTxConn<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.inner }
}

// with_immediate's closure now receives &mut WriteTxConn<'_>:
pub async fn with_immediate<R, F>(pool: &SqlitePool, f: F) -> anyhow::Result<R>
where
    F: for<'c> FnOnce(&'c mut WriteTxConn<'c>) -> BoxFuture<'c, anyhow::Result<R>> + Send,
    R: Send,
{
    let mut conn = pool.acquire().await?;            // PoolConnection<Sqlite>
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    // `pool.acquire()` returns `PoolConnection<Sqlite>` (a guard
    // that derefs to SqliteConnection); we need `&mut SqliteConnection`,
    // so the explicit `&mut *conn` reborrow is mandatory here:
    let mut wt = WriteTxConn::new(&mut *conn);
    /* …match on f(&mut wt).await as today, plus COMMIT/ROLLBACK on
       `&mut *conn` after f returns; the WriteTxConn drops at end
       of scope, releasing its inner borrow… */
}

// db/repositories/fiscal_documents.rs
pub async fn transition_state(
    conn: &mut WriteTxConn<'_>,           // <- newtype, was &SqlitePool
    id: DocumentId,
    from: DocState,
    to: DocState,
) -> sqlx::Result<TransitionOutcome>
```

Callers cannot construct `WriteTxConn` outside `db::tx` (sealed via the private `_seal` field), so the only way to obtain `&mut WriteTxConn<'_>` is to receive one from inside a `with_immediate` closure body.

**Pros:**
- **TRUE ENFORCEABLE AT COMPILE** for the PRRO_GATE-k99 enemy: `transition_state(&mut conn, …)` where `conn: SqliteConnection` is a hard type error.  No path around the seal short of `unsafe`.
- The CAS-vs-SELECT race window closes automatically — both statements share the connection (via `Deref` to the underlying `SqliteConnection`), hence share the BEGIN IMMEDIATE serialisation.
- Compound ops (transition + `audit_log::append` + `node_state::update`) become ergonomic: pass `&mut WriteTxConn<'_>` to all three.  Each helper's signature self-documents the requirement in its type.
- Composes uniformly to other transactional helpers (`shifts::transition`, future `offline_session::transition`, etc.).  One pattern, applied everywhere.
- Zero runtime cost — the newtype is a thin wrapper; `Deref` calls compile away.
- Aligns with W2 cert_refresher in spirit: that code writes inline via `&mut *conn` inside the closure; the newtype makes the same rule explicit when the writes go through a helper.

**Cons:**
- **Signature break.** Every existing transactional call site must update.  Inventory today (M2 closed):
  - `transition_state` at `rust/prro/src/db/repositories/fiscal_documents.rs:139` — single call site, no production callers (only tests).
  - `transition` at `rust/prro/src/db/repositories/shifts.rs:83` — single call site, no production callers.
  - `with_immediate` callers at `rust/prro/src/db/repositories/ingress_inbox.rs:67`, `rust/prro/src/services/cert_refresher.rs:292,365` — closures must accept `&mut WriteTxConn<'_>` instead of `&mut SqliteConnection`; inline `sqlx::query(…).execute(&mut *conn)` becomes `…execute(&mut **conn)` (one extra deref) or `…execute(conn.as_mut())` if a method is exposed.
  - M3a impl will add ~5-10 new call sites — they receive `&mut WriteTxConn<'_>` from the start.
- **sqlx ergonomics.** sqlx 0.8 macros (`query!`, `query_scalar`, `query_as`) accept anything that impls `sqlx::Executor<'_, Database = Sqlite>`.  `WriteTxConn` does NOT impl `Executor` directly; callers use `&mut **conn` (Deref to `&mut SqliteConnection`, which does impl Executor).  This is one extra `*` per call site — annoying but mechanical and grep-able in review.

**Why NOT a bare `&mut SqliteConnection` (rejected sub-option):**
A naked `&mut SqliteConnection` does not enforce the BEGIN IMMEDIATE precondition.  A caller can write
```rust
let mut conn = pool.acquire().await?;        // raw acquire, no BEGIN IMMEDIATE
transition_state(&mut conn, id, Prepared, Signed).await?;  // compiles & runs
```
which type-checks but leaves the CAS-and-SELECT race window open (both statements would auto-promote to separate WAL implicit write-txes).  The newtype seal is the difference between "ergonomically biased toward correct usage" and "structurally impossible to misuse".  For a fiscal-state subsystem this gap matters.

**Verdict:** `ENFORCEABLE AT COMPILE` — sealed-newtype variant.  Meets the PRRO_GATE-k99 acceptance criterion ("upgrades transition_state itself to wrap the unhappy path in with_immediate").

### 4.3 Option (c): helper opens its own micro-tx

**Idea.** Helper signature stays `&SqlitePool`-based; helper
internally opens its own `with_immediate` for the
CAS+disambiguating-SELECT pair.

```rust
pub async fn transition_state(pool: &SqlitePool, …) -> … {
    if !allowed_transition(from, to) { return Ok(Forbidden); }
    with_immediate(pool, |conn| Box::pin(async move {
        let res = sqlx::query("UPDATE … WHERE state = ?").execute(conn).await?;
        if res.rows_affected() == 1 { return Ok(Applied); }
        let exists = sqlx::query_scalar("SELECT 1 …").fetch_optional(conn).await?;
        Ok(if exists.is_some() { Conflict } else { NotFound })
    })).await
}
```

**Pros:**
- Closes PRRO_GATE-k99 inside the helper itself — the
  CAS-and-SELECT pair are now atomic.
- Call-site signature unchanged.
- No call-site churn.

**Cons:**
- **Compound ops break.** A typical M3a stage 3b sequence is:
  `transition_state(Prepared, Signed)` + `INSERT document_files`
  + `UPDATE node_state` + `INSERT audit_log` — all inside ONE
  `with_immediate` so they atomically commit-or-rollback.  If
  `transition_state` opens **its own** `with_immediate`, the
  caller must NOT also be inside `with_immediate` (nested BEGIN
  IMMEDIATE is a SQLite error per `tx.rs:3-6`).  Either:
  - the caller is OUTSIDE `with_immediate` and `transition_state`
    runs as a stand-alone tx — BUT THEN the audit_log + document_files
    + node_state writes are in a SEPARATE tx, breaking atomicity
    of stage 3b.  **This is a correctness regression vs
    Python.**  Python's `_stage_sign` at `write_path.py:737-773`
    explicitly atomises `update_state + add_file + log_events`.
  - the helper detects an outer tx and DOESN'T open a new one —
    requires runtime introspection sqlx doesn't provide.

- **The CAS-only path (happy path, single UPDATE statement) does
  not actually need a multi-statement tx wrapper.** SQLite's WAL
  mode auto-promotes a single `UPDATE` to a brief implicit write
  tx; the BEGIN IMMEDIATE + COMMIT bracket adds latency for no
  correctness benefit on the happy path.  Option (c) penalises
  every CAS hit.

- **For the unhappy path** (CAS miss → SELECT) option (c) does
  buy atomicity, but option (b) buys it AND lets us atomise
  with the rest of the compound op, which is what M3a wants.

**Verdict:** `ENFORCEABLE AT RUNTIME` (helper-managed) but
**REJECTED** for M3a — atomicity-with-compound-op trumps
helper-internal atomicity.

### 4.4 Decision — option (b), sealed-newtype variant

**Primary: option (b) with sealed newtype `WriteTxConn<'_>`.
Change `transition_state` to take `&mut WriteTxConn<'_>`.**

Rationale:
1. **Atomicity-with-compound-op preserved.** The Python
   `_stage_sign` pattern (transition + add_file + audit) maps
   directly: caller opens `with_immediate`, receives `&mut WriteTxConn<'_>`,
   passes that borrow into `transition_state` AND
   `audit_log::append` AND `document_files INSERT` — one
   commit, all-or-nothing.
2. **PRRO_GATE-k99 closed by construction.** The unhappy-path
   SELECT runs through the same connection (via Deref) inside
   the outer tx; no race window.
3. **TRUE compile-time enforcement.** The `_seal: ()` private
   field means only `db::tx::with_immediate` can construct a
   `WriteTxConn`.  A caller cannot reach `transition_state`
   without first being inside `with_immediate` — this is
   structural, not policy.  See §4.2 "Why NOT a bare `&mut SqliteConnection`"
   for the rejected weaker variant.
4. **Module doc-comment** at
   `rust/prro/src/db/repositories/fiscal_documents.rs:14-25`
   already pre-figures this design — the existing comment says
   M3 is "expected to call `transition_state` inside its own
   `db::tx::with_immediate` envelope".  The newtype makes that
   call-pattern the only structurally legal one.
5. **Composes uniformly.** `shifts::transition` at
   `rust/prro/src/db/repositories/shifts.rs:83` (parallel
   API, same `&SqlitePool` signature today) gets the same
   newtype treatment.  Future `audit_log::append_inside_tx`,
   `offline_session::transition_status`, etc. all share the
   pattern — one design, applied everywhere.

**M3a implementation must:**
- Add `WriteTxConn<'a>` to `rust/prro/src/db/tx.rs` per the
  sketch in §4.2 (sealed `_seal: ()` private field;
  module-private `fn new` constructor — NOT `pub(crate)`,
  which would let any in-crate caller bypass `with_immediate`;
  `#[cfg(test)] pub(super) fn new_for_test` only inside `db::tx`
  for test access; `Deref`/`DerefMut` to `SqliteConnection`).
- Change `with_immediate`'s closure signature from
  `for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, _>`
  to `for<'c> FnOnce(&'c mut WriteTxConn<'c>) -> BoxFuture<'c, _>`.
- Update `transition_state` signature
  (`fiscal_documents.rs:139`) and `transition` signature
  (`shifts.rs:83`) to take `&mut WriteTxConn<'_>`.
- Update the existing closure-style call sites at
  `ingress_inbox.rs:67`, `cert_refresher.rs:292,365` to match
  the new closure signature.  Inline `sqlx::query(…).execute(&mut *conn)`
  becomes `…execute(&mut **conn)` (one extra deref through the
  newtype) — mechanical refactor.
- Remove the "known limitation" doc-comment at
  `fiscal_documents.rs:16-25` once the migration lands.
- Add a **two-phase regression test** for atomicity.  The two
  phases serve different purposes; only Phase B runs in CI.

  **Phase A (BASELINE — local, NOT in CI; deterministic via
  injected pause).** A test-only build of `transition_state`
  with an explicit `tokio::time::sleep` (or a `tokio::sync::
  Notify` rendezvous point) wedged between the missed CAS
  UPDATE and the disambiguating SELECT.  A concurrent task
  deletes the row during the wedged window; the test asserts
  the Conflict→NotFound flip is reproducible **deterministically**
  (no probabilistic timing).  The injected-pause fork lives
  under `#[cfg(test)] #[path = "..."] mod transition_state_pre_fix;`
  — it is NEVER part of the shipping build.  The test is run
  once locally before the newtype lands; its purpose is to
  prove the race window is real, not to catch regressions.
  Mark the test `#[ignore]` (or remove it after the proof
  cycle) so CI does not run it; the proof obligation is
  documented in the M3a impl PR description, not in CI green.

  Without the injected pause the race is probabilistic and
  CI-flaky; injecting the pause makes the proof
  deterministic but couples to a test-only fork of the
  helper.  The latter is acceptable for a one-time proof
  obligation but unacceptable for ongoing CI — hence the
  out-of-CI placement.

  **Phase B (POST-FIX — ongoing CI, deterministic by
  construction).** Drive `transition_state` from inside
  `with_immediate`; spawn a concurrent deleter task; assert
  the deleter blocks on the RESERVED lock until commit and
  the `transition_state` outcome is consistent (Conflict if
  the row was alive at lock-acquire, NotFound if it was
  already gone).  This is deterministic because the SQLite
  RESERVED lock provides the ordering guarantee — the
  deleter cannot interleave between the UPDATE and SELECT
  while the closure holds the connection.  This is the
  ongoing regression test; it runs in CI on every build.

  Why two phases: a single POST-FIX test cannot prove the
  newtype's value because the lock is held — the deleter
  can never observe the race window even if it existed.
  The BASELINE phase is the proof obligation (one-time,
  local); the POST-FIX phase is the ongoing protection (CI).

**Fallback: option (a) (POLICY ONLY).**  If `WriteTxConn`
ergonomics cause unforeseen blockers (e.g. sqlx 0.8's
macro-generated code interacts badly with the `Deref` chain
in some edge case), fall back to option (a) and rely on
review.  The PRRO_GATE-k99 acceptance criterion accepts both
((a) wrap-at-callsite review, OR (b) harden the helper).

**REJECTION** of option (c): incompatible with the
compound-op pattern that Python's reference implementation uses
at `write_path.py:737-773`.  Helper-managed micro-tx breaks
atomicity with audit_log / node_state / document_files.

### 4.5 Cross-link to W0-1 transitions

Per W0-1 §2.1 transition matrix, every DocState transition needs
write-tx context for the compound op (transition + audit + side
effects).  No transition is "pure CAS, no other writes" — even
the simplest transition (`Prepared → Rejected`) co-writes
`audit_log` and possibly `ingress_inbox.status`.  This is why
option (b) (caller-owned tx) is the right shape: every caller
already opens `with_immediate` for its own writes; passing
`conn` to `transition_state` is the no-cost extension.

For ShiftState / OfflineSession status / NodeMode (§2.2-§2.4 of
W0-1), the same option-(b) shape applies if M3 grows analogous
`transition_*` helpers (per W0-1 §6.3 PROPOSED amendment to add
`allowed_transition` whitelists for those state machines).

---

## 5. Crypto/network boundary pattern catalogue

W2 cert_refresher (`rust/prro/src/services/cert_refresher.rs`)
is the reference precedent.  Three patterns emerge.

### 5.1 Pattern A: "Compute outside, persist inside"

**Description.** Compute the bytes/hash/signature OUTSIDE any
`with_immediate`.  Pass the resulting value into the closure by
move; the closure does only DB persistence.

**Reference precedent.**
- `rust/prro/src/services/cert_refresher.rs:289-323`
  (`in_place_refresh`): `let fingerprint = compute_fingerprint(&new_cert);`
  at line 291 PRECEDES `with_immediate(pool, move |conn| { … })`
  at line 292.  The closure binds `&fingerprint` already-computed.
- `rust/prro/src/services/cert_refresher.rs:362-471`
  (`key_roll_atomic`): same — `compute_fingerprint` at line 364
  precedes `with_immediate` at line 365.  All cert metadata
  (`parsed.ski_hex`, `parsed.valid_from`, etc.) is pre-computed
  outside `parse_cert_metadata` (line 251 — also outside) before
  the with_immediate opens.
- Python equivalent: `write_path.py:737` BEGIN IMMEDIATE
  PRECEDED by `crypto_provider.sign(...)` at ~660; the
  `signed_payload` is bound into the persist-tx by reference.

**M3a applicability.**
- **Stage 1 (acquire+validate):** N/A — no crypto/network input
  to compute.
- **Stage 3a→3b (sign):** **MANDATORY.** `prro::xml::build_canonical_xml`
  + `CryptoProvider::sign_cms_detached` BEFORE Stage 3b's
  `with_immediate`.  Pass `signed_payload: SignedCmsBytes`
  (and any pre-computed `signed_xml_sha256`, `payload_xml_sha256`
  digests) into the closure by move.
- **Stage 4a→4b (send):** **MANDATORY.** `DpsChannel::send_chk`
  BEFORE Stage 4b's `with_immediate`.  Pass `send_response`
  (and `kvt1_raw: Option<Vec<u8>>` if returned inline) into
  the closure by move.
- **Stage 5 (finalize):** N/A — no crypto/network input.

**When NOT to use.** When the computation result depends on a
DB row that itself is being written in the same tx (e.g. you
need to UPDATE the row first, then sign the canonical XML).
This shape does not occur in M3a per W0-1 §3 — sign always
precedes persist.

### 5.2 Pattern B: "Persist inside, send outside"

**Description.** Mark intent in DB INSIDE a tx (e.g. flip state
to a "staged-for-send" intermediate), release the lock, then
issue the network call.  The network result is then absorbed by
a SECOND tx (Pattern A's persist-inside).

**Reference precedent.**
- Python `write_path.py:790` (mark `→SENDING`) → release lock
  → `transport_client.send(...)` at `:806-820` → `:874` open
  new lock to mark `→SENT`.  This is THE Python gold standard
  for "persist intent, then act, then persist outcome".

**Subtle correctness note.**  Pattern B's "intent" state
(`SENDING` in Python) is a crash-resume signal: if the process
crashes between the intent-commit and the wire-send-returning,
the recovery loop sees `SENDING` and routes the doc to
`ERROR_RETRYABLE` rather than re-sending (DPS does not
deduplicate; re-sending is dangerous — `write_path.py:144-163`).

**M3a applicability.**
- **Stage 4 (send): MANDATORY.**  Python uses Pattern B
  (`write_path.py:786-803` — open `with_immediate`, CAS to
  `SENDING` with `expected_states=(PREPARED, SIGNED, ENCRYPTED)`,
  commit, then call `transport_client.send` outside the lock).
  Crash-resume protection at `write_path.py:144-165` —
  documents found in `SENDING` after a process restart are
  routed to `ERROR_RETRYABLE` for operator inspection, NOT
  re-driven, with the explicit comment **"DPS does not
  deduplicate — re-sending is dangerous"**.

  Earlier W0-2 drafts recommended Pattern A only for stage 4
  on the assumption that a single-process binary + PID lease
  removes the need for the SENDING marker.  That recommendation
  was withdrawn after senior review — even a single-process
  daemon CAN crash mid-wire; on restart the persisted state is
  the only signal the recovery loop has, and a SIGNED-state
  doc is structurally indistinguishable from "stage 4 hasn't
  started yet" vs "stage 4 sent the wire request and crashed
  before the reply landed".  The latter case re-driving is
  exactly the duplicate-send hazard Python's SENDING marker
  protects against.

  **M3a adopts Pattern B for stage 4.**  This requires:
  - A new DocState value `Sending` added to
    `rust/prro/src/db/models/enums.rs:29-42` (12 → 13 values).
  - A new migration (proposed `008_doc_state_sending.sql`)
    that extends the `fiscal_documents.state` CHECK constraint
    to include `'SENDING'`.
  - W0-1 §2.1 transition whitelist amendment (proposed
    additions): `Signed → Sending`, `Encrypted → Sending`
    (Checkbox-flow), `Sending → Sent`, `Sending → Kvt1`
    (inline KVT1 path), `Sending → ErrorRetryable`.  The
    direct `Signed → Sent` and `Encrypted → Sent` transitions
    become unreachable in M3a code paths but stay in the
    whitelist for backward compatibility (no production
    callers today).
  - Recovery rule for `Sending` (App::boot) per W0-3 §3:
    "found in Sending → CAS Sending→ErrorRetryable; do NOT
    auto-re-send; surface to operator via audit
    `crash_resume_sending_to_error_retryable`".  Mirror of
    Python `write_path.py:144-165`.
  - `Sending` joins the pending set in
    `fiscal_documents.rs:176` doc-comment + the
    `list_pending_for_fn` SQL clause.

**When NOT to use.** When the wire side-effect is naturally
idempotent (transport guarantees deduplication on a stable
request id).  DPS does NOT meet this bar — `write_path.py:148`
explicitly states so.  Therefore Pattern B is mandatory wherever
DPS is the backend.  Pattern A could in principle be used for a
hypothetical idempotent Checkbox-style backend, but no such
shape is in M3a scope; document the case if M3b extends to one.

### 5.3 Pattern C: "Stage-and-flip"

**Description.** INSERT a "staging" row inside `with_immediate`,
release, do work outside, re-acquire `with_immediate` to flip
the staging row to active.  Used when the staging-row INSERT
itself needs to be atomic against a sibling row's `active=0`
flip.

**Reference precedent.**
- `rust/prro/src/services/cert_refresher.rs:365-471`
  (`key_roll_atomic`).  All three operations (stage INSERT,
  active=0 flip, active=1 flip) are inside ONE
  `with_immediate` because the STAGE itself was already
  computed outside (Pattern A: `compute_fingerprint`,
  `parse_cert_metadata`).  Note: the W2 implementation does
  NOT split stage-and-flip across two `with_immediate`s —
  it puts ALL THREE inside one tx because the new cert bytes
  are pre-computed.  That makes W2 a pure Pattern A use.
- A canonical Pattern C (split across two with_immediates) is
  rarer.  An example in Python: `offline_sync.py:107-144`
  re-signs a corrected payload (`raw_signed = sign_raw(...)`
  at :108 — OUTSIDE any tx); then `BEGIN IMMEDIATE` at :119
  DELETEs the stale `document_files` rows and INSERTs the new
  ones.  This is structurally Pattern A (compute outside,
  persist inside).

**M3a applicability.**
- **Not strictly required for M3a happy path.**  The W0-1 §3
  pipeline is linear: each stage either Computes (no lock) or
  Persists (with_immediate); no stage requires a stage-then-flip.
- **M3b reservation.**  Offline session lifecycle (M3b) may
  need Pattern C for the
  `OFFLINE_LOCAL_ACK → SENT/ACK/REJECTED` transitions where a
  staging row marks "in-flight to DPS" and is flipped on
  return.  Defer pattern selection to W0-3 / M3b.

### 5.4 Pattern selection matrix (M3a stages)

| Stage | Pattern | Why |
|-------|---------|-----|
| Inbox accept | A (no foreign IO) | The "compute" half is canonicalisation + SHA-256, both pure-CPU; "persist" is the inbox INSERT inside `with_immediate` |
| 1 acquire+validate | (no crypto/network at all) | Pure DB stage |
| 3 sign | A | M2-handoff §1 W2 precedent; Python `write_path.py:737` precedent |
| 4 send | **B** | Mandatory per §5.2: DPS does not deduplicate (`write_path.py:148`); SENDING marker is the only crash-resume safety mechanism.  Requires new DocState::Sending + migration 008 + W0-1 §2.1 whitelist amendment.  See §5.2 "M3a applicability" for the full prerequisite list. |
| 5 finalize | A (no foreign IO) | Pure DB stage |

---

## 6. `App::boot` lock-discipline preview

Cross-link to W0-3 (App::boot reconciliation contract closing
PRRO_GATE-ah8).  W0-2 documents only the LOCK-discipline
aspects of boot; W0-3 owns the recovery action policy.

### 6.1 Boot operations classified by lock kind

| Boot operation | Lock kind | Rationale |
|----------------|-----------|-----------|
| Migration runner — apply pending DDL | Migration runner uses its own BEGIN IMMEDIATE per migration (Python `migrations/runner.py:81`); Rust `db::open_pool` calls `sqlx::migrate!()` which uses `Transaction` — equivalent | Schema changes need atomic tx; one per migration script |
| Read pending docs per FN — enumerate `state IN pending-set` rows for re-drive | **read-only** — `pool.acquire()` + `SELECT`, no `with_immediate` | Audit-only; no writes |
| Read `node_state.mode` per FN — decide if FN needs `STOP_MODE` flip | **read-only** | Audit-only |
| Mark shifts as ERROR if found mid-transition (e.g. CLOSING with no Z report) | **`with_immediate`** | State write + audit_log together |
| Re-evaluate node_state vs persisted last-known state (PRRO_GATE-ah8) | **`with_immediate`** | shift_state preservation + node_state UPDATE atomic |
| Reseed `next_lnd` from DPS (out of M3a scope) | (M3b — `with_immediate` per FN) | Coordinated multi-row UPDATE |
| Claim a per-FN lease (if leases survive process restart) | **`with_immediate`** | Lease table is write |
| Drop stale leases (lease_expires_at < now) | **`with_immediate`** | Bulk write |

### 6.2 Boot-time invariants W0-2 must surface

- **No crypto / no network during boot.** A boot pass must NOT
  call CMP, DPS, or the sidecar.  Recovery RPCs (e.g. DPS
  `query_by_local_identity` to confirm a `SENT` doc was
  received) belong to the post-boot reconciliation worker, not
  to `App::boot` itself.  Thus all boot writes are pure-DB
  Pattern-A-degenerate (no compute step, only persist).
- **No long transactions across multiple FNs.** Each per-FN
  recovery action gets its own `with_immediate`.  Boot must NOT
  open one giant tx that holds across all FNs — that would
  serialise the entire fleet on a single writer.
- **Boot-time read-only enumeration is fine without `with_immediate`.**
  WAL mode lets readers proceed concurrently.  Use a normal
  `pool.acquire()` for the enumeration pass.

### 6.3 Boundary with W0-3

W0-3 owns the **action policy** per-state (PREPARED → re-drive,
SENT → query DPS, etc.).  W0-2 owns only **where the lock
goes** for the boot writes that W0-3 prescribes.  At hand-off:

- W0-3 says "for `SENT` boot docs, query DPS first then flip
  state".
- W0-2 says: "the `query DPS` call is the network step
  (Pattern A — compute), and the state flip + audit_log goes
  into a single `with_immediate`".
- This means `App::boot` for SENT docs is structurally
  identical to M3a stage 4: wire I/O outside, persist inside.

### 6.4 Open questions deferred to W0-3

- Does App::boot run reconciliation **inline** during startup
  (blocking `/health/ready`) or hand off to a background
  worker?  Affects whether the per-FN `with_immediate` writes
  are sequential (inline) or interleaved with handler traffic
  (background).  **W0-3 to decide.**
- Per-FN ordering: must FNs be reconciled in a specific order?
  If yes, the lease table must be writable before any
  reconciliation starts.  **W0-3 to decide.**
- Crash-mid-boot semantics: a process killed during boot must
  resume cleanly.  Boot writes are individually idempotent
  (CAS on state) but the SET of writes is not.  **W0-3 to
  decide.**

---

## 7. Reviewer checklist

A future reviewer must re-verify the following if any spec
contract changes.

### 7.1 If `with_immediate` helper signature changes

- `rust/prro/src/db/tx.rs:11-36` updated.
- All call sites compiled (currently:
  `rust/prro/src/db/repositories/ingress_inbox.rs:67`,
  `rust/prro/src/services/cert_refresher.rs:292,365`).
- If the change adds a runtime guard (option (b) of §3.6),
  every public-API entry point in `prro::crypto::*` and
  `prro::transports::dps::*` must `debug_assert!` against the
  flag — verify exhaustively against the M2-handoff §2.1/§2.2
  trait method inventories.
- Static-scan test (W0-2 lint extension of W5) re-run against
  every fresh callsite.
- This document's §3 re-verified.

### 7.2 If `transition_state` signature changes

- `rust/prro/src/db/repositories/fiscal_documents.rs:139-170`
  updated.
- Module doc-comment at lines 14-25 updated (specifically the
  "deferred to M3" paragraph).
- All `transition_state` call sites in `rust/prro/src/services/`
  and `rust/prro/src/db/repositories/` re-checked: each is
  inside a `with_immediate` (or its caller is — option (b)
  guarantees this structurally).
- Regression test for atomicity of CAS+SELECT pair under
  concurrent delete/insert added.
- bd-issue PRRO_GATE-k99 closure note added with citation to
  the new helper line numbers.
- W0-1 §2.1 transition matrix re-verified (no DocState change
  invalidated by the helper rewrite).
- This document's §4 re-verified.

### 7.3 If a `with_immediate` site is added or removed

- Static-scan lint output re-checked (option (c) / (d) of §3).
- §1 audit table updated with the new site.
- §2 stage table re-checked: does the new site match a stage
  contract, or is it a new stage?

### 7.4 If the boundary pattern catalogue changes

- §5 patterns re-checked vs W2 cert_refresher precedent
  (`rust/prro/src/services/cert_refresher.rs:289-471`).
- Stage selection matrix at §5.4 updated.
- Python audit (§1.1, §1.2) re-run for new drift.

### 7.5 If App::boot semantics change

- W0-3 spec consulted (action policy is W0-3 territory; lock
  policy is W0-2).
- §6.1 boot-operation table re-checked.
- §6.2 invariants re-checked — particularly "no crypto / no
  network during boot".
- This document's §6 re-verified.

### 7.6 If invariant #1 itself changes

- M2-handoff §4.1 re-verified.
- CLAUDE.md "Frozen invariants" §1 re-verified.
- All sections of this document re-litigated — invariant #1 is
  the spine of the spec.

---

## 8. Proposed ADR amendments

The following amendments to
`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` are
**PROPOSED — NOT COMMITTED**.  Coordinator to surface to user
for approval before any edit.

### 8.1 PROPOSED — NOT COMMITTED — ADR-M3-A3: `with_immediate` enforcement (hybrid)

```
Decision: M3a adopts a HYBRID enforcement strategy for the
"no foreign IO inside with_immediate" rule (operationalising
M2-handoff §4.1 invariant #1):

(1) Keep the existing F: ... + Send bound on
    rust/prro/src/db/tx.rs:13.  Necessary but not sufficient.

(2) Add a syn-based static-scan test (sibling of
    rust/prro/tests/api_surface_no_db_handle.rs) that walks
    every with_immediate(...) closure body and denies .await
    paths matching the M2 substrate method-name denylist
    (sign_cms_detached, verify_dstu, unwrap_envelope,
    fetch_cert_by_ski, send_chk, last_chk, ping, status_rro,
    info_rro, query_by_local_identity, by_server_fiscal_no).

(3) Add a tokio::task_local! IN_WITH_IMMEDIATE in
    rust/prro/src/db/tx.rs.  with_immediate enters via
    IN_WITH_IMMEDIATE.scope((), async { f(&mut wt).await }) so
    the marker follows the future across .await boundaries on
    the multi-threaded tokio runtime.  Public-API entry points
    of prro::crypto::* and prro::transports::dps::*
    debug_assert!(IN_WITH_IMMEDIATE.try_with(|_| ()).is_err(),
    "foreign IO inside with_immediate"); compiles out in
    release, panics loudly in debug + tests.

    Why NOT thread_local!: with_immediate is async; the future
    can migrate between tokio worker threads after .await, and
    spawn_blocking targets land on dedicated blocking-pool
    threads with separate thread-local tables — a thread_local!
    flag would not reach the assertion site.

(4) Document that arbitrary helper-fn-of-helper-fn chains
    remain POLICY ONLY (review-only) — no automated
    enforcement covers indirection past the named callees.

Rationale: option (a) (compile-time enforcement of the
forbidden-IO invariant) is POLICY ONLY for arbitrary async fn
calls — Rust's type system has no negative auto-traits for
"forbids reqwest/tonic/tokio::net".  Option (b) (runtime
debug_assert!) enforces only at known callees; option (c)
(static scan) catches direct callsites only.  Hybrid catches
the most ground.

Research-addresses: PRRO_GATE-k99 reinforcement (no direct
closure — the helper-side hardening for the bd-issue is
ADR-M3-A4 below).  Bd-issue closure deferred to M3a
implementation time.
```

### 8.2 PROPOSED — NOT COMMITTED — ADR-M3-A4: tx-witness newtype + transition_state signature change

```
Decision: M3a introduces a sealed newtype `WriteTxConn<'a>` in
rust/prro/src/db/tx.rs whose constructor is private to db::tx
(only with_immediate builds it).  with_immediate's closure
signature changes to receive `&'c mut WriteTxConn<'c>` instead
of `&'c mut SqliteConnection`.

All transactional repository helpers that today take
&SqlitePool change signature to take &mut WriteTxConn<'_>:
- rust/prro/src/db/repositories/fiscal_documents.rs:139
  transition_state
- rust/prro/src/db/repositories/shifts.rs:83 transition
- (M3a-introduced) audit_log::append, etc.

Pre-condition for callers: structurally enforced by the seal —
&mut WriteTxConn<'_> can only be obtained from inside a
with_immediate closure.  This is the canonical pattern
(mirrors the Python compound-op pattern at
src/prro_gateway/services/write_path.py:737-773 — transition +
add_file + audit_log atomic).

PRRO_GATE-k99 closes by construction: the CAS-miss
disambiguating SELECT inherits the outer tx (via Deref to the
underlying SqliteConnection), so the race window between
UPDATE and SELECT-1 collapses.  See W0-2 §4.4 "two-phase
regression test" for the proof obligation: a BASELINE test
must demonstrate the race exists on the pre-fix &SqlitePool
API before the newtype lands; an ongoing POST-FIX test asserts
the window is closed.

Module doc-comment at fiscal_documents.rs:14-25 ("known
limitation deferred to M3") removed when the migration lands.

Rationale: option (b) sealed-newtype variant of W0-2 §4.
Bare &mut SqliteConnection (rejected sub-option) does NOT
enforce the BEGIN IMMEDIATE precondition — see W0-2 §4.2 "Why
NOT a bare &mut SqliteConnection".  WriteTxConn's _seal: ()
private field is the structural guarantee; without it, option
(b) collapses to STRONG CONVENTION rather than compile-time
enforcement.

Migration cost: existing callers updated in lockstep
(currently zero production transition_state / transition
callers; only tests + closure-style with_immediate sites at
ingress_inbox.rs:67, cert_refresher.rs:292,365 — mechanical
&mut **conn refactor for inline sqlx::query sites).

Lifetime-shape caveat: the proposed HRTB
`for<'c> FnOnce(&'c mut WriteTxConn<'c>) -> BoxFuture<'c, _>`
quantifies a single 'c that applies to BOTH the outer mutable
reference AND the inner connection borrow inside WriteTxConn.
This is shape-equivalent to the existing `tx.rs:13`
`for<'c> FnOnce(&'c mut SqliteConnection)` HRTB and should
compile, but the M3a impl must verify on a real call site
that the borrow checker accepts the WriteTxConn variant
without extra lifetime gymnastics.  Two backup shapes if it
does not:
- `for<'a, 'c> FnOnce(&'c mut WriteTxConn<'a>) -> BoxFuture<'c, _> where 'a: 'c`
  — separate inner/outer lifetimes with subtyping bound.
- `for<'c> FnOnce(WriteTxConn<'c>) -> BoxFuture<'c, _>`
  — by-value transfer; closure consumes WriteTxConn (with the
  inner `&mut SqliteConnection` borrow still alive for 'c);
  helpers take `&mut WriteTxConn<'_>` from a local variable.
Pick whichever the borrow checker accepts cleanest at impl
time.

Fallback: option (a) (POLICY ONLY) if WriteTxConn ergonomics
cause unforeseen blockers (e.g. sqlx 0.8 macro interaction
with the Deref chain on `&mut **conn`, or HRTB lifetime
shape unworkable across all three options above).

Research-addresses: PRRO_GATE-k99 (bd-issue closure deferred
to M3a implementation time, not at this research close).
```

### 8.3 PROPOSED — NOT COMMITTED — ADR-M3-A5: boundary-pattern selection per pipeline stage

```
Decision (architectural — pattern selection only;
implementation details for SENDING / Pattern B live in
ADR-M3-A9 to avoid duplication):

M3a write-path uses:
- Pattern A ("Compute outside, persist inside"; W0-2 §5.1) at
  stage 3 sign and any other foreign-IO stage where the wire
  side-effect is naturally idempotent.
- Pattern B ("Persist intent, act, persist outcome"; W0-2 §5.2)
  at stage 4 send.  Mandatory because DPS does NOT deduplicate
  — Python `write_path.py:148` explicitly states "DPS does not
  deduplicate — re-sending is dangerous", and Python's
  SENDING-marker pattern at `write_path.py:786-803` +
  `:144-165` is the gold-standard crash-resume protection.
  Adopting Pattern A only for stage 4 would create a real
  duplicate-send hazard at DPS on any process crash between
  state=SIGNED commit and the wire reply landing — even a
  single-process daemon can crash, and recovery has no way to
  distinguish "stage 4 hasn't started" from "stage 4 sent the
  wire request and crashed before the reply" without the
  SENDING marker.
- Pattern C ("Stage and flip") reserved for M3b (offline
  lifecycle).

Pattern selection is the binding contract; the M3a
implementation contract for the Pattern B SENDING state +
recovery rule + whitelist + migration lives in **ADR-M3-A9**
(W0-3 §8.4) to keep the architectural decision (this ADR) and
the implementation contract (A9) separately auditable.  This
ADR REQUIRES A9 to land alongside it.

Research-addresses: invariant #1 reinforcement (M2-handoff
§4.1) + invariant #4 idempotency (CLAUDE.md "Frozen
invariants" — at-the-wire idempotency is preserved by Pattern
B even when DPS itself does not deduplicate).
```

### 8.4 No-op note

If user rejects 8.1, 8.2, or 8.3, bd-issue PRRO_GATE-k99
remains open and M3a implementation must re-litigate (likely
falling back to option (a) of §4.4 — POLICY ONLY review).
This document remains the research artefact regardless.

**Use "Research-addresses: PRRO_GATE-k99 (bd-issue closure
deferred to M3a implementation time)" — NOT "Closes:".**

---

## 9. Test acceptance contract (M3a impl gate)

The contracts in §3 (`with_immediate` enforcement hybrid),
§4 (`transition_state` + WriteTxConn newtype), and §5 (boundary
patterns) are not credible without explicit test obligations.
This section names the negative + positive fixtures M3a impl
MUST land before any of A3 / A4 / A5 are considered enforced
in code.  Each test category is sized — these are not
research-only acceptance criteria, they are M3a impl gates.

### 9.1 `with_immediate` foreign-IO guardrail tests (ADR-M3-A3)

The hybrid enforcement (Send bound + static scan + task_local
runtime guard) must be backed by all five cases below — two
static-scan gates (#1 substrate methods, #3 ad-hoc
`spawn_blocking`), two runtime-guard gates (#2 indirect
helper-of-helper, #4 provider public-API entry positive
control), and one negative control (#5):

| # | Case | Test kind | Expected outcome |
|---|------|-----------|------------------|
| 1 | **Direct M2-substrate `.await` inside `with_immediate` closure** — `with_immediate(pool, |c| Box::pin(async move { provider.sign_cms_detached(req).await; … }))`.  `provider.sign_cms_detached` is on the M2-substrate denylist | Static-scan test (W5 sibling lint extension; cargo test target) | Compile / cargo test FAILS with the lint name + the offending call site path |
| 2 | **Indirect foreign-IO via helper-of-helper (runtime catch)** — `with_immediate(pool, |c| Box::pin(async move { local_helper(c).await; … }))` where `local_helper(c)` itself calls `crypto.sign_cms_detached`.  Static scan misses this because the call expression in the closure body is the (allowed) `local_helper`, not the (denied) substrate method.  The runtime guard catches it AT THE PUBLIC API ENTRY of `sign_cms_detached` — which is `async fn` polled in the awaiting task's context where the task-local IS visible.  The panic happens BEFORE the provider dispatches into its internal `spawn_blocking` | Runtime test (`tokio::test(flavor = "multi_thread")` — uses real `InProcessProvider`) | Test panics with `foreign IO inside with_immediate`; debug-assert message names the offending provider method (e.g. `sign_cms_detached`); a provider-spy verifies ZERO `spawn_blocking` dispatches happened (proving the panic fires at entry, not somewhere deeper) |
| 3 | **Direct `tokio::task::spawn_blocking` ad-hoc inside `with_immediate` closure** — `with_immediate(pool, |c| Box::pin(async move { tokio::task::spawn_blocking(|| heavy_crypto()).await; … }))`.  Bypasses the M2 substrate entirely; `spawn_blocking` body runs on a blocking-pool thread WITHOUT the task-local visible (tokio does not propagate task-local table into spawn_blocking closures — the closure is a synchronous `FnOnce`, not a future).  The runtime guard CANNOT see inside the closure body; the static scan catches the literal `spawn_blocking` call expression in the closure AST | Static-scan test (W5 sibling lint denylist includes `spawn_blocking` and `block_in_place` literal call expressions, in addition to the M2 substrate method names) | Compile / cargo test FAILS with the lint name + the offending call site path; **NOT a runtime test** — the runtime cannot deterministically catch this case (see §3.6 "What `tokio::task_local!` does NOT cover") |
| 4 | **Provider public-API entry positive control** — `with_immediate(pool, |c| Box::pin(async move { provider.sign_cms_detached(req).await; … }))` driven through real `InProcessProvider` to verify the runtime guard fires at the entry of the substrate method itself (not inside its internal `spawn_blocking` body, which would never trip).  This proves task_local is visible at the entry-time `debug_assert!` | Runtime test (`tokio::test(flavor = "multi_thread")`) | Test panics with `foreign IO inside with_immediate`; debug-assert message names `sign_cms_detached`; provider-spy verifies the panic happens BEFORE any internal `spawn_blocking` was dispatched (zero `block_in_place_count` / zero `spawn_blocking_count` observed) |
| 5 | **Negative control — substrate call OUTSIDE `with_immediate`** — same `provider.sign_cms_detached` call but in regular async code, not inside any `with_immediate` closure | Runtime test | Call succeeds; no panic; the task-local guard is `IN_WITH_IMMEDIATE.try_with(|_| ()).is_err()` (i.e. the scope is empty) |

If any of (1) (2) (3) (4) does not panic / fail as specified
above, the enforcement contract is broken and the M3a hybrid
is **not** doing its job.  Cases (1) and (3) are static-scan
gates — failure means the W5-sibling lint is incomplete.
Cases (2) and (4) are runtime-guard gates — failure means
the `tokio::task_local!` setup or the provider-side
`debug_assert!` placement is incorrect.  Case (5) is the
negative control — failure means false positive on the
guard.  Fall back to ADR-M3-A3 fallback ("static scan
alone, runtime guard removed") only if cases (2) (4) prove
unreliable; document the regression.

### 9.2 `transition_state` + WriteTxConn compile-fail tests (ADR-M3-A4)

The newtype seal is only meaningful if the type system actually
rejects the cases the seal exists to forbid.  Use either
`trybuild` (preferred — established Rust pattern for
compile-fail tests) or doc-tests with `compile_fail` annotation.

| # | Case | Test kind | Expected outcome |
|---|------|-----------|------------------|
| 1 | **Caller passes raw `&mut SqliteConnection` to `transition_state`** — `let mut conn: SqliteConnection = …; transition_state(&mut conn, doc_id, Prepared, Signed).await;` | trybuild compile-fail | `error[E0308]: mismatched types ... expected &mut WriteTxConn ..., found &mut SqliteConnection` |
| 2 | **Caller tries to construct `WriteTxConn::new` from outside `db::tx`** — in any module other than `db::tx`: `let wt = WriteTxConn::new(&mut *conn);` | trybuild compile-fail | `error[E0603]: function "new" is private` |
| 3 | **Caller tries to construct `WriteTxConn` via the struct literal syntax** — `let wt = WriteTxConn { inner: &mut *conn, _seal: () };` from outside `db::tx` | trybuild compile-fail | `error[E0451]: field "_seal" of struct "WriteTxConn" is private` |
| 4 | **Valid usage** — closure-style `with_immediate(pool, |wt| Box::pin(async move { transition_state(wt, …).await })).await?;` | Regular `cargo test` (compile + run) | Compiles; transition_state CAS completes; no panic |
| 5 | **`#[cfg(test)] new_for_test` works inside `db::tx` only** — test in `db::tx::tests` calls `WriteTxConn::new_for_test(&mut *conn)`; test in any other module fails to find the symbol | One trybuild compile-fail (other module) + one regular test (inside `db::tx`) | Both as expected |

These tests are the **structural proof** that PRRO_GATE-k99
is closed by construction.  Without them, the seal is
documentation, not enforcement.

### 9.3 Two-phase atomicity test for `transition_state` (already specified in §4.4)

The Phase A (BASELINE — local, NOT in CI; deterministic via
injected pause) + Phase B (POST-FIX — ongoing CI) split is
defined in §4.4.  Restated here for completeness:

- **Phase A** is run once locally before the newtype lands;
  `#[ignore]` in CI; proof obligation documented in the M3a
  impl PR description.
- **Phase B** runs in CI on every build; deterministic via the
  SQLite RESERVED lock ordering.

### 9.4 Boundary-pattern smoke tests (ADR-M3-A5)

Per pattern catalogued in §5:

| # | Pattern | Test target | Acceptance |
|---|---------|-------------|------------|
| 1 | Pattern A (compute outside, persist inside) — stage 3 sign | M3a stage-3 happy-path test that drives the pipeline through `Prepared → Sending`-or-`Signed`; asserts `CryptoProvider::sign_cms_detached` is called BEFORE the lock opens (capture-spy provider records call timestamp; lock-open audit captures wall-clock; assert `sign_call_ts < lock_open_ts`) | `sign_call_ts < lock_open_ts` — proves the hoist |
| 2 | Pattern B (mark intent, send, persist outcome) — stage 4 send | M3a stage-4 happy-path test — drives `Signed → Sending → wire-send → Sending → Sent` via tonic mock (M2 W3); asserts the SENDING CAS commits BEFORE `DpsChannel::send_chk` is called (capture-spy DpsChannel records call timestamp; SENDING-commit audit captures timestamp; assert `sending_commit_ts < send_chk_call_ts`) | `sending_commit_ts < send_chk_call_ts` — proves Pattern B intent-marker comes first |
| 3 | Pattern B crash-resume — SENDING → ErrorRetryable | M3a App::boot test — pre-seed a doc with `state=SENDING`; run `App::boot`; assert (a) the doc transitions to `ErrorRetryable`, (b) audit `crash_resume_sending_to_error_retryable` is logged, (c) the DpsChannel mock records ZERO `send_chk` invocations for the doc id | All three assertions hold |

