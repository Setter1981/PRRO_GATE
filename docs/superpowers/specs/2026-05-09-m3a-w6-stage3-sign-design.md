# M3a W6 — Stage 3 Sign (Pattern A) — Design Spec

**Date:** 2026-05-09
**Status:** Approved for apply (conditional GO received after v5)
**Anchors:** ADR-M3-A2, ADR-M3-A5, W0-1 §3.3, W0-2 stage 3a/3b
**Predecessor:** W5 (PR #24, merged `ca0357a`) — `WorkerContext` + stage 1+2 acquire/validate/guard
**Successor:** W7 (stage 4 send / Pattern B with SENDING marker)

---

## 1. Anchors

| Anchor | Source : line | Constraint |
|---|---|---|
| **ADR-M3-A2** | `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md:940-965` | SHIFT_CLOSE → ZReport at builder boundary; Z-allocation MUST key on `wire_artifact_kind`, NOT on internal `DocType` label; "fire correctly for both SHIFT_CLOSE and Z_REPORT internal labels" |
| **ADR-M3-A5 (Pattern A)** | `docs/superpowers/specs/2026-05-06-m3-w0-2-lock-discipline.md:888-928` | Compute outside, persist inside. Stage 3a→3b "MANDATORY" Pattern A. Pass `signed_payload`, pre-computed digests by move into the closure |
| **W0-1 §3.3** | `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md:576-596` | Pre-cond `state==PREPARED`; outside-lock = `build_canonical_xml` + `sign_cms_detached`; inside-lock = `transition_state(Prepared,Signed)` + `document_files INSERT(SIGNED_XML)` + optional `INSERT(PAYLOAD_XML)` for DPS + audit; **Hand-off note (W0-1 original)**: prevhash chain reads `last_known_unsigned_xml_sha256` OUTSIDE any tx; persistence into `node_state` deferred to stage 5. **W6 intentionally amends read timing**: seed read happens INSIDE the 3-PRE write tx (alongside `pin_signing_inputs_tx` + optional `allocate_z_report_number`) to pin signing inputs atomically and turn pin-once into a structural property — see §2 3-PRE step (d) and §11 invariant #4 strengthening. The W0-1 "outside any tx" rationale (seed is immutable post-ACK) is preserved as a *correctness invariant* (atomic snapshot inside 3-PRE is a strict refinement of "non-racing read"); the timing change is a deliberate refinement, not a regression. |
| **W0-2 stage 3a/3b** | `docs/superpowers/specs/2026-05-06-m3-w0-2-lock-discipline.md:152-153` | **3a (no lock)**: build XML, resolve prevhash MAC, `sign_cms_detached`. **3b (with_immediate)**: CAS `Prepared→Signed` + `INSERT SIGNED_XML` + optional `INSERT PAYLOAD_XML` + audit. Hoisted bytes pre-computed: `signed_payload`, `signed_xml_sha256`, `payload_xml_sha256` |
| **Pattern A precedent** | `rust/prro/src/services/cert_refresher.rs:289-323` | `compute_fingerprint` at :291 PRECEDES `with_immediate` at :292 — exact shape stage_sign mirrors |
| **W3 enforcement** | `tests/with_immediate_no_foreign_io.rs` (W3 PR #23) | static-scan denylist already covers `sign_cms_detached`; runtime guard panics if called inside `with_immediate`. **Stage 3 will be one of the cases this scanner is now actively enforcing.** |

---

## 2. Pattern A boundary mapping (3 sub-stages)

```
worker entry (W5 WorkerContext{document.state==Prepared}, no lock)
  │
  ├─ STAGE 3-PRE-READ — POOL read, no lock
  │    tax_number = fiscal_number_config::get(pool, fn).tax_number
  │    (tax_number is config-table data; not part of chain pinning;
  │     keep 3-PRE write tx as short as possible.)
  │
  ├─ STAGE 3-PRE — INSIDE with_immediate (short write envelope)
  │    Goal: ensure signing inputs are PERSISTED on the doc BEFORE sign,
  │          so a sign-fail / crash post-allocation can retry without
  │          gap or re-allocation.
  │
  │    a. derive wire_artifact_kind  (pure CPU, ADR-M3-A2 gate)
  │
  │    b. SELECT state, signing_inputs_pinned_at, previous_hash, z_report_number
  │         FROM fiscal_documents WHERE document_id = ?
  │
  │    c. STATE GATE: if state != Prepared:
  │         return Err(SignError::StateConflict { observed: state, doc_id })
  │         — NO pin, NO Z allocation, NO audit row.  Caller decides
  │           resume / abort / surface.  Closes stale-WorkerContext race.
  │
  │    d. PIN BRANCH:
  │       if signing_inputs_pinned_at IS NULL:
  │         seed = node_state.last_known_unsigned_xml_sha256  (Option<[u8;32]>)
  │         prev = seed                                        (raw bytes, NOT hex)
  │         z = if wire_artifact_kind == ZReport:
  │                Some(node_state::allocate_z_report_number(tx, fn))
  │             else:
  │                None
  │         pin_signing_inputs_tx(tx, doc_id, prev.as_ref(), z)
  │           — single UPDATE writing previous_hash, z_report_number,
  │             signing_inputs_pinned_at = CURRENT_TIMESTAMP atomically
  │           — WHERE state = 'PREPARED' AND signing_inputs_pinned_at IS NULL
  │             (pin-once guard; idempotent under concurrent re-entry)
  │         audit "sign_inputs_pinned" {seed_was_none: prev.is_none(),
  │                                     z_allocated: z.is_some()}
  │       else:  (already pinned: REUSE)
  │         prev = previous_hash    (Option<[u8;32]> from row)
  │         z    = z_report_number  (Option<i64> from row)
  │         — NO seed re-read, NO Z re-allocation
  │
  │    COMMIT.  Outputs (by move into 3-NO-TX phase):
  │      previous_hash_raw: Option<[u8;32]>,
  │      z_report_number: Option<i64>,
  │      tax_number: String, doc_lnd: i64, fn: String, business_ts: String,
  │      command (W5 had), payload typed via serde
  │
  ├─ STAGE 3-NO-TX — OUTSIDE any lock
  │    1. ts_str = format_kyiv_local(business_ts)            ← chrono-tz
  │    2. typed_payload = parse_command_payload_json::<TypedPayload>(...)
  │       (serde, fail-closed; mismatch → SignError::PayloadSchema)
  │    3. local_number = u32::try_from(doc_lnd)
  │       .map_err(|_| SignError::Range { field: "lnd", value: doc_lnd })?
  │       z_number_u32 = match z_report_number {
  │         Some(z) => u32::try_from(z)
  │           .map_err(|_| SignError::Range { field: "z_report_number", value: z })?,
  │         None    => 0,   // Python parity: <DAT ZN="0"> for non-Z artifacts
  │                          // (xml/mod.rs:382-383 always emits ZN attribute)
  │       }
  │    4. previous_hash_hex = previous_hash_raw
  │         .map(hex_encode)
  │         .unwrap_or_default()   // "" when unpinned-empty (genuine first-doc)
  │    5. canonical_doc = build_canonical_doc(
  │         wire_artifact_kind,
  │         DocumentHeader::with_defaults(fn, tax_number, z_number_u32,
  │                                       ts_str, previous_hash_hex),
  │         local_number, typed_payload)
  │    6. unsigned_xml = build_canonical_xml(&canonical_doc)?
  │    7. unsigned_xml_sha256 = sha2::Sha256::digest(unsigned_xml)
  │    8. signed_payload = ctx.provider.sign_cms_detached(SignCmsRequest{
  │         session: &ctx.session,
  │         canonical_xml: &unsigned_xml,
  │         profile: ctx.profile,
  │       }).await?
  │       ← async; spawn_blocking inside provider; FORBIDDEN inside any tx
  │         (W3 static scan + runtime guard enforce)
  │
  └─ STAGE 3-PERSIST — INSIDE with_immediate (single envelope)
       a. transition_state(tx, doc_id, Prepared, Signed)
            CAS; Forbidden/Conflict/NotFound surface as errors (no silent skip)
            — final authoritative state gate; even if 3-PRE state check
              passed, a concurrent finalize could race; CAS catches it
       b. document_files::insert_tx(tx, doc_id, PayloadXml, &unsigned_xml)
            (M3a is DPS-only per W0-1 §3.3)
       c. document_files::insert_tx(tx, doc_id, SignedXml, &signed_payload.0)
       d. update_unsigned_xml_sha256_tx(tx, doc_id, &unsigned_xml_sha256)
            (W5 inserted NULL; canonical write per design)
       e. audit_log::append_tx "doc_signed" (Info)
       COMMIT.

Outcome: Result<SigningOutcome { document: DocumentRow{state==Signed, …},
                                  signed_payload, unsigned_xml }, SignError>
```

**Why a separate pre-sign tx vs folding into persist tx?** Retry after crypto failure must observe the **same** `previous_hash` AND `z_report_number` so re-build → re-sign produces the **same** canonical bytes (M2 W4 byte-equiv contract). Pinning-before-sign turns retry-safety from "allocator must be deterministic" into "allocator runs once, persisted". Z-retry fixture #4 below proves it.

**Why state gate in 3-PRE?** A stale `WorkerContext` (built when state was Prepared, but state has since transitioned via concurrent finalize/reject) would otherwise pin `previous_hash` + advance `next_z_report_number` before 3-PERSIST CAS rejects. Early gate avoids consuming Z numbers and pinning inputs for a doc whose flow already concluded. CAS at 3-PERSIST remains as the second authoritative gate.

**Crypto-degraded hysteresis flip** (`node_state.mode CRYPTO_DEGRADED→ONLINE`): out of W6 scope.

---

## 3. Inputs from W5 WorkerContext — gap analysis

`WorkerContext` (`services/write_path/types.rs:32-52`) carries: `inbox`, `command`, `node_state`, `active_shift`, `document`.

| Need (DocumentHeader / CanonicalDoc field) | Source | Resolution |
|---|---|---|
| `fiscal_number` | `inbox.fiscal_number` / `node_state.fiscal_number` | direct |
| `tax_number` | NOT in WorkerContext | **read `fiscal_number_config` BEFORE 3-PRE (pool read)** — not part of chain-pinning invariant |
| `z_number` | none persisted today | **persist on `fiscal_documents.z_report_number`** (new column per migration 009); allocate-or-reuse in 3-PRE; non-Z → `0` (Python parity) |
| `ts_str` Kyiv-local `YYYYMMDDHHMMSS` | none | **`chrono-tz` Europe::Kiev** (new dep); 3-NO-TX |
| `previous_hash` (raw 32 bytes for DB; hex for XML) | `node_state.last_known_unsigned_xml_sha256: Option<[u8;32]>` | hex-encode for `<MAC>` attr if Some, empty `""` if None (Python parity, `xml/mod.rs:62-63`); persist raw bytes into `fiscal_documents.previous_hash` (BLOB(32) per `migrations/002:31`) in 3-PRE so retry sees the same bytes |
| `device_name` / `device_version` | constants | `DocumentHeader::with_defaults` defaults `"ПРО_каса"` / `"1.1"` |
| `local_number` (`<DAT DI>`) | `document.lnd` | **`u32::try_from(lnd)` checked**; fail-closed `SignError::Range` |
| typed `items`/`payments`/`opening_sum`/etc. | `command.payload_json` + `command.total_sum_kop` | **typed serde structs, fail-closed**; private helper in `stage_sign.rs`; mismatch → `SignError::PayloadSchema` |
| `signing_ctx` (`provider: Arc<dyn CryptoProvider>`, `session: SigningSession`, `profile: CmsProfile`) | NOT in WorkerContext | **passed by dispatcher**; W6 does NOT load cert/session |

---

## 4. Artifact / persistence model

| In-memory | Persisted? | Where | Stage |
|---|---|---|---|
| `wire_artifact_kind` | NO (in-memory) | enum derived from `DocType` × `ShiftState` at the DocType→CanonicalDoc mapping point | 3-PRE |
| `unsigned_xml: Vec<u8>` (cp1251 canonical body) | YES | `document_files.kind='PAYLOAD_XML'` | 3-PERSIST |
| `unsigned_xml_sha256: [u8;32]` | YES | `fiscal_documents.unsigned_xml_sha256` (UPDATE NULL→hash) | 3-PERSIST |
| `signed_payload: SignedCmsBytes` (CMS detached SignedData DER) | YES | `document_files.kind='SIGNED_XML'` | 3-PERSIST |
| `previous_hash: [u8;32]` (raw, NOT hex) | YES | `fiscal_documents.previous_hash` (BLOB; UPDATE NULL→raw bytes) | 3-PRE |
| `signing_inputs_pinned_at: TEXT` | YES | `fiscal_documents.signing_inputs_pinned_at` (NULL=not pinned, ISO8601=pinned) | 3-PRE |
| `z_report_number: u32` | YES | `fiscal_documents.z_report_number` (NEW column; UPDATE on first allocation, REUSE on retry) | 3-PRE |
| `node_state.next_z_report_number` | YES | NEW column; UPDATE next+1 RETURNING next-1 | 3-PRE allocator (advance once per ZReport doc) |
| `node_state.last_known_unsigned_xml_sha256` | NO write here | stage 5 finalize advances chain seed on ACK; W6 only **READS** | (out-of-scope read in 3-PRE) |

**Z-allocation rule (verbatim ADR-M3-A2 + safety amendment):**
> Z-number is allocated AT MOST ONCE per fiscal_documents row. Allocation persists `fiscal_documents.z_report_number` AND advances `node_state.next_z_report_number` atomically inside the 3-PRE tx. Retry observes the persisted `z_report_number` and reuses it; the allocator IS NOT called again. Partial UNIQUE index `ux_fd_fn_zrn` is the fail-closed downstream guard.

**z_number serialization for non-Z artifacts:**
> `xml/mod.rs:382-383` always emits the `ZN` attribute on `<DAT>`. For non-Z `wire_artifact_kind` (`Sell`/`Return`/`ShiftOpen`), runtime stage_sign sets `header.z_number = 0`, serializing as `<DAT ... ZN="0">` (Python parity). The Z-counter is meaningful only for ZReport artifacts; runtime fixture #6b locks the ZN=0 expectation.

---

## 5. Repo surface deltas

### 5.1 New migration 009

```sql
-- 009 — Z-report sequencer + signing-inputs pin marker (W6).
-- Adds:
--   node_state.next_z_report_number  (allocator state)
--   fiscal_documents.z_report_number (persisted allocation; retry reuse)
--   fiscal_documents.signing_inputs_pinned_at (pin-once flag; disambiguates
--     "not pinned yet" from "pinned with empty/None previous_hash")
--   ux_fd_fn_zrn                     (partial UNIQUE; fail-closed)

ALTER TABLE node_state
  ADD COLUMN next_z_report_number INTEGER NOT NULL DEFAULT 1
  CHECK (next_z_report_number >= 1);

ALTER TABLE fiscal_documents
  ADD COLUMN z_report_number INTEGER
  CHECK (z_report_number IS NULL OR z_report_number >= 1);

ALTER TABLE fiscal_documents
  ADD COLUMN signing_inputs_pinned_at TEXT;

CREATE UNIQUE INDEX ux_fd_fn_zrn
  ON fiscal_documents(fiscal_number, z_report_number)
  WHERE z_report_number IS NOT NULL;
```

SQLite `ALTER ADD COLUMN` with constant DEFAULT + CHECK is supported (verified vs SQLite 3.45 docs); does NOT need table-rebuild like 008. Net new disk = 1 INTEGER per node_state row + 2 nullable per fiscal_documents row + small partial index.

### 5.2 New file `src/db/repositories/document_files.rs`

```rust
#[derive(Clone, Copy, Debug, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum DocumentFileKind {
    #[sqlx(rename = "PAYLOAD_XML")]            PayloadXml,
    #[sqlx(rename = "SIGNED_XML")]             SignedXml,
    #[sqlx(rename = "KVT1_RAW")]               Kvt1Raw,
    #[sqlx(rename = "KVT2_RAW")]               Kvt2Raw,
    #[sqlx(rename = "PAYLOAD_JSON_CANONICAL")] PayloadJsonCanonical,
    #[sqlx(rename = "RECEIPT_PDF")]            ReceiptPdf,
}

pub async fn insert_tx(tx: &mut WriteTxConn<'_>, doc_id: DocumentId,
                       kind: DocumentFileKind, content: &[u8])
    -> sqlx::Result<()>;

pub async fn get_tx(tx: &mut WriteTxConn<'_>, doc_id: DocumentId,
                    kind: DocumentFileKind)
    -> sqlx::Result<Option<Vec<u8>>>;
```

PRIMARY KEY (document_id, kind) — INSERT-or-error.

### 5.3 `src/db/repositories/node_state.rs` — additive

```rust
// (1) Extend NodeStateRow with next_z_report_number.
pub struct NodeStateRow {
    /* existing 8 fields */
    pub next_z_report_number: i64,    // NEW
}

// (2) BOTH `get` and `get_tx` SELECTs add `next_z_report_number` to keep
//     `cargo sqlx prepare` cache in sync (forgetting this = silent drift).

// (3) Allocator — bare CAS UPDATE for parity with W5 `allocate_next_lnd`.
//     The existing `node_state_updated_at` trigger
//     (migrations/001_core_identities.sql:76-79) covers updated_at.
pub async fn allocate_z_report_number(tx: &mut WriteTxConn<'_>, fn_id: &str)
    -> sqlx::Result<i64>
{
    sqlx::query_scalar(
        "UPDATE node_state
            SET next_z_report_number = next_z_report_number + 1
          WHERE fiscal_number = ?
         RETURNING next_z_report_number - 1"
    )
    .bind(fn_id)
    .fetch_one(&mut **tx)
    .await
}
```

### 5.4 `src/db/repositories/fiscal_documents.rs` — additive

```rust
pub struct DocumentRow {
    /* existing 9 fields */
    pub previous_hash: Option<[u8; 32]>,            // NEW (raw bytes, NOT hex)
    pub z_report_number: Option<i64>,               // NEW
    pub unsigned_xml_sha256: Option<[u8; 32]>,      // NEW
    pub signing_inputs_pinned_at: Option<String>,   // NEW (NULL = not pinned)
}

pub struct PinnedSigningInputs {
    pub state: DocState,                  // NEW — pre-pin state gate
    pub is_pinned: bool,                  // signing_inputs_pinned_at IS NOT NULL
    pub previous_hash: Option<[u8; 32]>,
    pub z_report_number: Option<i64>,
}

/// W6 stage 3-PRE — read state + pin status atomically inside the envelope.
pub async fn get_signing_inputs_tx(
    tx: &mut WriteTxConn<'_>, doc_id: DocumentId,
) -> sqlx::Result<Option<PinnedSigningInputs>>;

/// W6 stage 3-PRE — pin signing inputs onto the doc row.
/// Single atomic UPDATE writing previous_hash, z_report_number,
/// signing_inputs_pinned_at = CURRENT_TIMESTAMP.
/// WHERE-guarded: state = 'PREPARED' AND signing_inputs_pinned_at IS NULL.
/// rows_affected == 0 means state moved OR already pinned; caller has
/// the get_signing_inputs_tx read in same tx and acts on truth.
pub async fn pin_signing_inputs_tx(
    tx: &mut WriteTxConn<'_>, doc_id: DocumentId,
    previous_hash: Option<&[u8; 32]>,
    z_report_number: Option<i64>,
) -> sqlx::Result<u64>;  // rows_affected

/// W6 stage 3-PERSIST — UPDATE the now-canonical unsigned hash.
pub async fn update_unsigned_xml_sha256_tx(
    tx: &mut WriteTxConn<'_>, doc_id: DocumentId, hash: &[u8; 32],
) -> sqlx::Result<bool>;
```

**Affected SELECTs that return DocumentRow MUST add 4 new columns + decode helpers** for the 2 BLOB-32 columns (`previous_hash`, `unsigned_xml_sha256`):
- `list_pending_for_fn`
- `get_pending_by_request_id_tx`

Shared decode helper colocated in `fiscal_documents.rs`: `fn decode_blob32(raw: Option<Vec<u8>>) -> Result<Option<[u8;32]>, sqlx::Error>` (mirrors `node_state::decode_chain_hash` pattern, 32-byte fail-closed).

### 5.5 `src/services/write_path/stage_sign.rs` — NEW

```rust
/// Signing context — injected by the worker dispatcher.  W6 does NOT
/// touch cert_provisioning / cert_refresher / session unsealing.
pub struct SigningContext {
    pub provider: std::sync::Arc<dyn CryptoProvider>,
    pub session: crate::crypto::session::SigningSession,
    pub profile: prro_crypto::cms::profile::CmsProfile,
}

pub enum SignError {
    StateConflict { observed: DocState, document_id: DocumentId },
    PayloadSchema { detail: String },
    Range { field: &'static str, value: i64 },
    Crypto(CryptoError),
    Build(XmlBuildError),
    TimestampConversion(/* chrono-tz parse / out-of-range */),
    Db(sqlx::Error),
}

pub struct SigningOutcome {
    pub document: DocumentRow,         // state == Signed, hashes pinned
    pub signed_payload: SignedCmsBytes,
    pub unsigned_xml: Vec<u8>,         // for downstream stage 4 wire send
}

pub async fn run(
    pool: &SqlitePool,
    ctx: &SigningContext,
    incoming: WorkerContext,
) -> Result<SigningOutcome, SignError>;
```

Private helpers:
- `derive_wire_artifact_kind(doc_type) -> WireArtifactKind` (enum: ShiftOpen, Sell, Return, ZReport)
- `parse_payload(payload_json, wire_artifact_kind) -> TypedPayload` (serde, fail-closed)
- `build_canonical_doc(kind, header, local_number, payload) -> CanonicalDoc`
- `format_kyiv_local(business_ts) -> String` (chrono-tz Europe/Kiev → `YYYYMMDDHHMMSS`)
- `hex_encode(bytes: &[u8]) -> String` (existing pattern from W5 stage_acquire)

`pub mod stage_sign;` added to `services/write_path/mod.rs`.

### 5.6 No churn elsewhere

- `with_immediate` / `WriteTxConn` — untouched.
- `transition_state` whitelist — `Prepared→Signed` already admitted at `fiscal_documents.rs:88`.
- `xml::build_canonical_xml` / `CanonicalDoc` / `DocumentHeader` — untouched (M2 W4 frozen artefact).
- `CryptoProvider` trait — untouched (consumed only).
- W5 `stage_acquire.rs` and W5 ingress logic — untouched apart from DocumentRow expansion (see §10 step 3).

### 5.7 Cargo.toml

- `+ chrono-tz = "0.10"` (1 line)
- `sha2 = "0.10"` already at `Cargo.toml:47` — no add

---

## 6. Tests `tests/write_path_stage3_sign.rs`

### 6.1 Spy crypto provider + atomic order counter

```rust
struct SpyCrypto {
    sign_call_seq: Arc<AtomicUsize>,
    persist_first_stmt_seq: Arc<AtomicUsize>,
    counter: Arc<AtomicUsize>,
    sign_result: Arc<Mutex<Result<SignedCmsBytes, CryptoError>>>,
}

impl CryptoProvider for SpyCrypto {
    async fn sign_cms_detached(&self, _req: SignCmsRequest<'_>) -> Result<...> {
        self.sign_call_seq.store(self.counter.fetch_add(1, AcqRel)+1, Release);
        self.sign_result.lock().unwrap().clone()
    }
    /* others: unimplemented!() — out of stage_sign reach */
}
```

**Pattern A timestamp ordering proof:**
`sign_call_seq.load() != 0 && sign_call_seq.load() < persist_first_stmt_seq.load()`.

Stage 3-PERSIST records `persist_first_stmt_seq` as its first inside-tx statement via a debug-only hook gated by `#[cfg(any(test, feature = "test-hooks"))]`. Production path remains identical.

Replaces audit-timestamp scheme (second-granular wall clock); atomic counter gives strict ordering.

### 6.2 Required fixtures (8)

| # | Name | Asserts |
|---|---|---|
| 1 | `stage3_happy_path_sell` | input PREPARED Sell doc (W5 happy state); spy `sign_cms_detached` called once with `canonical_xml == build_canonical_xml(CanonicalDoc::Sell(...))`; outcome doc.state==Signed; both `document_files` rows present (PAYLOAD_XML, SIGNED_XML); `unsigned_xml_sha256` UPDATEd from NULL; `previous_hash IS NULL` (no chain seed at boostrap); `z_report_number IS NULL`; `signing_inputs_pinned_at IS NOT NULL`; audit `doc_signed` once; **Pattern A proof**: `sign_call_seq < persist_first_stmt_seq` |
| 2 | `stage3_z_allocation_for_both_internal_labels` (sub-cases ShiftClose / ZReport) | both reach CanonicalDoc::ZReport build branch; `allocate_z_report_number` called exactly once per case; `fd.z_report_number = N`, `node_state.next_z_report_number = N+1`; non-Z DocTypes (Sell/Return/ShiftOpen) DO NOT touch the allocator; `<DAT ZN="N">` for Z, `<DAT ZN="0">` for non-Z |
| 3 | `stage3_crypto_error_leaves_doc_prepared_no_files_and_reuses_pinned_inputs` (parameterised Z / non-Z) | spy returns `CryptoError::SignFailure`; outcome `Err(SignError::Crypto(...))`; doc.state == Prepared (no transition); `document_files` empty for this doc; `unsigned_xml_sha256` stays NULL. **Z case**: `fd.z_report_number = 1`, `node_state.next_z_report_number = 2` (3-PRE committed before sign failed); `signing_inputs_pinned_at IS NOT NULL`. **Non-Z case**: same except `z_report_number IS NULL` and `next_z_report_number` unchanged. Safety property: **inputs pinned, ready for retry to reuse** |
| 4 | `stage3_z_retry_reuses_persisted_z_number` | first run: ZReport doc, sign fails, pre-sign tx committed → `fd.z_report_number = 1`, `next_z_report_number = 2`. Second run on same doc with successful spy: assert allocator NOT called; built CanonicalDoc.header.z_number == 1; sign succeeds; `next_z_report_number` STILL = 2 (no second advance); `fd.state == Signed` |
| 5 | `stage3_persist_rollback_on_post_sign_db_error` | inject failure at `document_files INSERT(SIGNED_XML)` (orchestrate via deleting fd row between 3-NO-TX and 3-PERSIST OR a synthetic test-only constraint). Assert: persist tx rolls back; doc.state stays Prepared; no PAYLOAD_XML row leak; pre-sign tx state (z_report_number, previous_hash, signing_inputs_pinned_at) preserved (separate envelopes by design) |
| 6a | `xml_builder_byte_equiv_w4_golden_zn_nonzero` (pure builder, NOT stage_sign) | feed CanonicalDoc::Sell with `header.z_number=7` ⇒ byte-equiv vs M2 W4 golden corpus. Sanity guard against M2 W4 frozen drift |
| 6b | `stage3_runtime_byte_equiv_zn_zero_for_nonz_artifact` (stage_sign runtime) | run full stage_sign on a Sell doc with deterministic inputs; assert built `unsigned_xml` contains literal substring `ZN="0"`; `unsigned_xml_sha256` deterministic across two runs of the same fixture |
| 7 | `stage3_first_doc_no_seed_then_seed_appears_retry_signs_with_empty_prev_hash` | first run on a doc whose `node_state.last_known_unsigned_xml_sha256 IS NULL` (true bootstrap); sign fails; pre-sign tx committed → `fd.previous_hash IS NULL`, `signing_inputs_pinned_at IS NOT NULL`. **Mutate** `node_state.last_known_unsigned_xml_sha256` to a non-NULL value. Retry: assert (a) 3-PRE goes the **else-branch**, does NOT re-read seed; (b) `fd.previous_hash` STILL IS NULL; (c) built canonical XML header `<MAC>` is empty; (d) sign succeeds; (e) doc.state == Signed |
| 8 | `stage3_stale_workercontext_returns_state_conflict_no_pin_no_z_alloc` | seed a fd row that ALREADY transitioned to `'SIGNED'` directly; pass a stale WorkerContext (built when state was Prepared) into `stage_sign::run`. Assert: returns `Err(SignError::StateConflict { observed: Signed, .. })`; `signing_inputs_pinned_at` still NULL on the row; `next_z_report_number` unchanged; no `document_files` rows; no `audit_log` row "sign_inputs_pinned" |

### 6.3 W3 scanner regression

Static scan over `services/write_path/stage_sign.rs` MUST stay green — `sign_cms_detached` is in the W3 denylist. Confirms W6 doesn't accidentally place `.sign_cms_detached(...)` inside `with_immediate`.

---

## 7. Non-goals (explicit)

W6 does NOT touch:

- `DpsChannel::send_chk` — W7 (stage 4)
- `DocState::Sending` transition — W7 (Pattern B intent-marker)
- Stage 5 finalize (`Kvt2→Ack`, `node_state.last_known_unsigned_xml_sha256` UPDATE for chain seed advance)
- `DpsError` routing — W10
- `App::boot` recovery — W8
- Hysteresis flip `node_state.mode CRYPTO_DEGRADED→ONLINE`
- **Cert / session loading** — `SigningContext` injected by dispatcher

---

## 8. Open questions (all resolved)

| ID | Resolution |
|---|---|
| OQ-A | ✅ Migration 009: `node_state.next_z_report_number` + `fiscal_documents.z_report_number` (NULLable) + `fiscal_documents.signing_inputs_pinned_at` (NULL=unpinned) + partial UNIQUE `ux_fd_fn_zrn`. Z persisted on doc, retry reuses, advance happens once. |
| OQ-B | ✅ inline private helper, typed serde structs, fail-closed; `SignError::PayloadSchema` on mismatch. JSON shape pinned in fixtures. If shape disputed → separate ingress-adapter follow-up, NOT W6. |
| OQ-C | ✅ `chrono-tz = "0.10"` add |
| OQ-D | ✅ 3 sub-stages: 3-PRE-READ (pool tax_number) + 3-PRE write tx (state gate, pin-once branch) + 3-NO-TX (build+sign) + 3-PERSIST (CAS+files+sha256). Stage 5 (out of scope) owns `node_state.last_known_unsigned_xml_sha256` advance. |
| OQ-E | ✅ sha2 already at `Cargo.toml:47` — no add |
| OQ-F | ✅ rollback fixture (#5) + Z-retry fixture (#4) + first-doc-empty-prev fixture (#7) + stale-state fixture (#8) all included |
| Hysteresis | ✅ out of W6 |
| Signing context injection | ✅ `SigningContext { provider, session, profile }` is `stage_sign::run` parameter |
| Pattern A proof | ✅ `AtomicUsize` order counter, NOT audit timestamps |
| `previous_hash` storage type | ✅ raw `[u8; 32]` (BLOB), hex-encoded only at the XML boundary |
| Pin-once + state gate | ✅ `signing_inputs_pinned_at` flag + `WHERE state='PREPARED' AND signing_inputs_pinned_at IS NULL` UPDATE guard + 3-PRE early state check |
| Checked u32 casts | ✅ `u32::try_from(...)` + `SignError::Range`; no `as u32` |
| z_number for non-Z | ✅ `0` (Python parity, `<DAT ZN="0">`); fixture #6b locks runtime expectation |

---

## 9. Repo footprint

| File | Lines (rough) |
|---|---|
| `migrations/009_z_report_seq.sql` (NEW) | ~26 |
| `src/db/repositories/document_files.rs` (NEW) | ~80 |
| `src/db/repositories/node_state.rs` | ~45 |
| `src/db/repositories/fiscal_documents.rs` | ~135 |
| `src/db/repositories/mod.rs` | 1 |
| `src/services/write_path/stage_sign.rs` (NEW) | ~300 |
| `src/services/write_path/mod.rs` | 1 |
| `src/services/write_path/stage_acquire.rs` (DocumentRow literal at :267) | +4 fields |
| `tests/write_path_stage1_acquire.rs` (suppression helper at :837) | +4 fields |
| `tests/write_path_stage3_sign.rs` (NEW) | ~640 |
| `Cargo.toml` | +1 (chrono-tz) |
| `.sqlx/` | regenerate |

**~1070 LoC code+tests.** Plan budget 3-4 days holds.

---

## 10. Apply ordering (single commit, W-task pattern)

1. **Migration 009** — 3 ALTER ADDs + 1 partial UNIQUE; `cargo sqlx prepare` regen
2. **`node_state` repo** — extend `NodeStateRow` with `next_z_report_number`; both `get` and `get_tx` SELECTs add the column; `allocate_z_report_number` (bare CAS UPDATE…RETURNING)
3. **`fiscal_documents` repo — DocumentRow expansion**:
   - Extend `DocumentRow` with 4 new fields (`previous_hash: Option<[u8;32]>`, `z_report_number: Option<i64>`, `unsigned_xml_sha256: Option<[u8;32]>`, `signing_inputs_pinned_at: Option<String>`).
   - Update ALL DocumentRow callsites (3 total):
     - `src/services/write_path/stage_acquire.rs:267` — W5 stage 1 inline literal; add 4 fields, all `None`.
     - `tests/write_path_stage1_acquire.rs:837` — `_unused_imports_suppression` helper; add 4 fields, all `None`.
     - 2 SELECTs in `fiscal_documents.rs` (`list_pending_for_fn`, `get_pending_by_request_id_tx`) — extend SELECT + decode helper for BLOB-32 columns.
   - Add `pin_signing_inputs_tx`, `get_signing_inputs_tx` (returning `PinnedSigningInputs`), `update_unsigned_xml_sha256_tx`.
   - Verify with `cargo build -p prro` — compiler exhaustively flags any callsite missed.
4. **`document_files` repo** — new file with `DocumentFileKind` enum + `insert_tx` + `get_tx`; `mod.rs` wiring.
5. **`stage_sign.rs`** — `SigningContext`, `SignError`, `SigningOutcome`, `run`. Flow: 3-PRE-READ (pool fiscal_number_config) → 3-PRE write tx (state gate → pin-once branch / reuse branch) → 3-NO-TX (typed serde parse, Kyiv-local TS, checked u32 casts, build_canonical_xml, sha2, sign_cms_detached) → 3-PERSIST (CAS, document_files inserts, sha256 update, audit). `mod.rs` wiring.
6. **Cargo.toml** — `+ chrono-tz = "0.10"`.
7. **Tests** — 8 fixtures + spy with `AtomicUsize` order counter + Pattern A proof; W3 scanner regression check.
8. **Gate**:
   - `cargo fmt -p prro -- --check` clean
   - `cargo clippy -p prro --all-targets --no-deps -- -D warnings` clean
   - `cargo test -p prro --test write_path_stage3_sign` 8/8
   - `cargo test -p prro --test with_immediate_no_foreign_io` 8/8 (W3 stays green)
   - `cargo test -p prro --locked` full, 0 failed
   - `.sqlx/` cache committed

---

## 11. Invariants (PRRO frozen list)

| # | Invariant | W6 preservation |
|---|---|---|
| 1 | No network/crypto inside SQLite write transaction | ✅ Pattern A: `sign_cms_detached` strictly between 3-PRE COMMIT and 3-PERSIST BEGIN. W3 static scan + runtime guard cover the new module. |
| 2 | One `fiscal_number` = one logical single-writer write-path | ✅ unchanged — W5 lease CAS still gates worker entry. |
| 4 | Idempotency mandatory | ✅ strengthened — pin-once UPDATE guard (`WHERE state='PREPARED' AND signing_inputs_pinned_at IS NULL`); retry deterministically re-uses pinned `previous_hash` + `z_report_number`; sign re-execution produces byte-equivalent CMS (M2 W4 contract). |
| 6 | Adapters build full canonical payloads | ✅ typed serde parser (fail-closed on schema mismatch); raw JSON does not enter the XML builder. |
| 7 | All canonical envelopes carry `schema_version` | N/A in W6 (envelope construction at ingress; W6 receives prebuilt `payload_json`). |
| 8 | Recovery and reconciliation must not silently violate state transitions | ✅ strengthened — 3-PRE state gate explicitly returns `SignError::StateConflict` before any pin/allocation; 3-PERSIST CAS provides second authoritative gate. No silent retries or auto-advance. |
| 9 | Graceful shutdown matters more than finishing fast | ✅ both write tx envelopes (3-PRE, 3-PERSIST) are short; long-running work is in 3-NO-TX where graceful shutdown can interrupt without DB lock. |

---

**Status:** Spec frozen at v5. Awaits explicit `GO apply` to begin implementation in worktree `m3a/W6-stage3-sign`.
