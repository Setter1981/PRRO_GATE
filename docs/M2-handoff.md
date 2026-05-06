# M2 Handoff — Rust crypto + transport substrate

**Status:** M2 epic closed (PRRO_GATE-82j); **M2 code baseline:
`66e317c` on `origin/rust-gateway`** — this handoff document is
the M2 contract revision cited by M3. M3 not yet started.

**Purpose of this document:** freeze the public contracts and
operational decisions M2 has already taken, so M3 (write-path
stages) can build on top instead of re-litigating them. Anything
in §2 ("Frozen public contracts") is *not negotiable inside M3* —
changes require an ADR amendment or an explicit handoff revision,
not a refactor commit.

Companion docs:
- ADR: `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` (six ADRs M2-1..M2-6, approved 2026-05-04).
- W0 findings: `2026-05-04-m2-w0-1-dps-wire.md` (DPS = gRPC), `…-w0-2-cmp-probe.md` (CMP = lookup-by-SKI), `…-w0-3-prro-crypto-audit.md` (CryptoProvider trait shape).
- Plan: `docs/superpowers/plans/2026-05-04-m2-w1-implementation.md` (+`.tasks.json`).
- Goldens: `docs/M2-goldens-capture.md`.

---

## 1. What M2 closed (W1–W6) + the tests that prove it

Every W landed via implementer + spec-reviewer + code-quality-reviewer
gate; review fix-commits are listed where they were required.

### W0 — research (3 sub-tasks)

W0-1/2/3 specs above. **Verified inputs frozen 2026-05-04.**
- DPS wire = gRPC, not Tax-XML/HTTP. Canonical proto vendored from `src/prro_gateway/transports/proto/fiscal_server.proto`.
- CMP = IIT lookup-by-SKI via `prro_crypto::cms::cmp::fetch_cert_by_ski`. No enrolment/rekey/revocation in M2 scope.
- `prro_crypto` API sufficient; `CryptoProvider` trait shape pinned; redacted-Debug discipline mandatory for secret-bearing types.

### W1 — `prro::crypto` in-process wrapper

**Commits:** `731ba2e` substrate · `e1c63e1` smoke (5 tests).

**Lands:**
- `CryptoProvider` async trait (no DB handle) — methods: `sign_cms_detached`, `verify_dstu`, `unwrap_envelope`, `fetch_cert_by_ski`. ADR-M2-1/-6. Full signature in §2.1.
- `SigningSession` + `SealedMaterial` with **redacted manual `Debug` impl** (NOT `#[derive(Debug)]`); plaintext private key wrapped in `Zeroizing<[u8; 32]>` (zeroize crate's runtime wrapper, NOT `#[derive(Zeroize)]`); intermediate plaintext password / decoded sealed bytes also held in `Zeroizing<Vec<u8>>` / `Zeroizing<String>` and dropped before unseal returns. ADR-M2-5.
- `unseal_jks(SealedMaterial) → Result<SigningSession, CryptoError>` helper.
- `InProcessProvider` (stateless `Copy`, per-call delegation via `tokio::task::spawn_blocking`).
- `CryptoError` enum (5 variants) with reason-typed kinds: `JksUnseal { reason: SealKind }`, `CmsSign { reason: SignKind }`, `EnvelopeDecrypt { reason: DecryptKind }`, `CertFetch { reason: FetchKind }`, `VerifyFailed { reason: VerifyKind }`. Manual `Debug` impl on `CryptoError` (forward-compat for redacted fields).

**Test surface:** `rust/prro/tests/crypto_provider_smoke.rs` — 5 tests including a real `verify_dstu` round-trip against a positive-fixture DSTU triple from `prro_crypto::core::sign::verify`.

### W2 — `services::cert_refresher`

**Commits:** `aebc9f0` (prereq: `prro_crypto::cms::envelope::parse_cert_basic_fields`) · `06c9ce1` migration 006 + seam · `9a8c16c` types/helpers · `0fe55aa` body · `971d1d8` ski_hex bind fix · `5ede215` invariant #1 hoist fix · `7abe5b7` wiremock smoke.

**Lands:**
- Migration `006_ca_endpoints.sql` (reuses M1 `cert_provisioning_config.timeout_seconds`).
- `services::cert_refresher::refresh_for_fn` returning `Result<RefreshOutcome, RefreshError>` — **no `Failed`-as-`Ok` variant**.
- Multi-URL fallback with per-URL timeout (method param; no provider config).
- Same-SKI **in-place UPDATE** writes `valid_from/valid_to/subject_dn/issuer_dn`; key-roll via **ONE `with_immediate` tx** containing stage (`INSERT … ON CONFLICT(ski_hex) DO UPDATE … WHERE same fn AND active=0`) + flip + audit. Both UPDATEs guarded with `rows_affected==1`; mismatch → ROLLBACK.
- `compute_fingerprint` hoisted **above** `with_immediate` (invariant #1: no crypto inside DB tx).
- `hex_to_ski` fail-closed via `MalformedCertMetadata` typed error.
- Stale-read guard: deactivate UPDATE binds `ski_hex`.

**Test surface:** `rust/prro/tests/cert_refresher_smoke.rs` — wiremock-backed byte-replay HTTP server in place of live CMP endpoint; vendored test-CA fixture.

### W3 — `prro::transports::dps` gRPC channel

**Commits:** `52349f0` proto seam · `9b6e19e` trait + DTOs · `c5e4684` query_by_local_identity + ServerFiscalIdMismatch · `42f02ba` real RPC bodies + status mapping · `580ed20` grpc-timeout per request · `cbcf371` native tonic mock + 17 integration tests · `f51616b` capture outbound proto bodies.

**Lands:**
- `tonic-build` + `protoc-bin-vendored` build dep (CI independent of system protoc).
- `rust/prro/proto/fiscal_server.proto` vendored from Python tree.
- `DpsChannel` async trait, **no DB handle** anywhere in public API.
- `DpsError` 8 variants (incl. `ServerFiscalIdMismatch`, `QueryNotSupported`).
- `GrpcDpsChannel` with `request<T>` helper that sets `grpc-timeout` HTTP/2 metadata header per RPC via `tonic::Request::set_timeout`.
- Default impls on the trait: `by_server_fiscal_no` = `lastChk(fn_sign) + response.id match` (PRRO_GATE-5js).

**Test surface:** `rust/prro/tests/dps_channel_smoke.rs` — native Rust tonic mock server via `tokio_stream::TcpListenerStream` ephemeral-port listener; 17 tests covering 5 happy paths with full proto-body assertions, 5 status mappings, 2 `tonic::Status` flavours, 3 `ByServerFiscalNo` shapes, `QueryNotSupported`, and grpc-timeout-on-every-RPC.

### W4 — byte-equivalence goldens harness

**Commits:** `fd81b03` scope fix (CloseShift == Z_REPORT) · `8d43882` W4-C0 contract (Python = oracle, Rust = candidate; **ADR-M2-3**) · `ddcd390` canonical-XML builder + cp1251 encoder · `732844f` review fix (port Python shapes literally) · `2a61f8e` Python `regenerate.py` + frozen fixtures · `c5cefba` byte-equiv harness · `4462cbd` review fix (full-overlap diff scan).

**Lands:**
- `rust/prro/src/xml/mod.rs` — hand-written canonical XML builder for **4 doc types**: `ShiftOpen`, `Sell`, `Return`, `ZReport`. **No separate ShiftClose** — WebCheck CloseShift IS DPS Z_REPORT (typCheck=2, doctype=80).
- `rust/prro/src/xml/cp1251.rs` — full Windows-1251 mapping including Ukrainian (Ї/І/Є/Ґ), №, smart quotes.
- `rust/prro/tests/goldens/regenerate.py` — manual-only Python capture, deterministic `BUSINESS_TS = 2026-05-06 12:00:00 Kyiv-local` → wire TS `20260506120000`.
- 6 first-round goldens: 4 XML + 1 CMS deterministic prefix + 1 prevhash seed.
- Harness: full-overlap diff scan (not first-64 only); window centred on first-diff offset (±32 bytes); missing-fixture = hard panic with regenerate.py remediation message; **no skip-if-missing**.
- KVT1/KVT2 fixtures **deferred** from first round (no parser in W1; follow-up gated on M3).

**Test surface:** `rust/prro/tests/goldens_byte_equiv.rs` — 7 tests including `xml_z_report_byte_equivalent_doubles_as_close_shift`.

### W5 — ADR-M2-6 static check

**Commits:** `79c8fe8` syn-based scan · `7dec811` review fix (ImplTrait/TraitObject/BareFn coverage + proper cfg(test) parsing + duplicate-name resolution).

**Lands:**
- `rust/prro/tests/api_surface_no_db_handle.rs` — syn 2 + quote 1 dev-deps.
- AST-based scan of `crypto::` + `transports::` public APIs (free fns AND trait method defs); fails on `SqlitePool`, `SqliteConnection`, `Pool`, `Transaction`.
- Type-tree walker covers Path/Reference/Box/Arc/Tuple/**ImplTrait/TraitObject/BareFn**/AssocType (closes obfuscation paths: `impl Trait`, `dyn Trait`, `Box<dyn FnMut(SqlitePool)>`).
- `has_cfg_test` proper meta parsing: `cfg(test)`, `cfg(all(test,…))` skip; `cfg(not(test))`, `cfg(any(test,…))` do NOT skip.
- `services/` exempt (carve-out per ADR-M2-6).
- 16 tests including negative fixtures for free-fn AND trait-method violations + anchor-sanity for `unseal_jks`/`sign_cms_detached`/`send_chk`/`connect`/`verify_dstu`.

### W6 — secret-material flow tracing test

**Commits:** `09b1835` substrate · `11176d5` review fix (global subscriber + multi-encoding canaries + happy-state Debug coverage) · `66e317c` doc-scrub Cargo.toml comment.

**Lands:**
- `rust/prro/tests/secret_flow_tracing.rs` — implements **ADR-M2-5 §4d** test contract.
- `tracing-subscriber 0.3` (std + fmt only) dev-dep; self-managed `Arc<Mutex<Vec<u8>>>` `MakeWriter` (no `tracing-test::internal::*` private API).
- **Process-global subscriber via `set_global_default` + `OnceLock`** — captures emissions from `tokio::task::spawn_blocking` blocking thread pool, not just current_thread runtime.
- Test serial via `tokio::sync::Mutex` (std Mutex across `.await` would trip clippy `await_holding_lock`).
- 3 seeded canaries: password, cred_salt, 32-byte private key.
- 6 private-key needle representations: lowercase hex, UPPERCASE hex, decimal Debug `[222, 173, 190, 239, …]`, `{:02x?}` Debug `[de, ad, be, ef, …]`, spaced hex, colon hex.
- Drives every `CryptoError` variant + Debug-prints `SigningSession` and `SealedMaterial`.
- Cross-thread positive control: `spawn_blocking` emits seeded canary, test proves global subscriber captures it.
- Per-needle positive detector via `catch_unwind` — proves matcher is not fictional.

**Test surface:** 3 tests in `secret_flow_tracing.rs`.

### M2 totals

- **22 integration test suites · 164 tests** in `prro` crate, all passing on `66e317c`.
- Cargo gates green: `cargo fmt -p prro -- --check`, `cargo clippy -p prro --all-targets --no-deps -- -D warnings`, `cargo test -p prro`.
- ADR gates: ADR-M2-1 (in-process wrapper), -2 (gRPC + tonic mock), -3 (Python serializer = byte-oracle), -4 (cert refresher async + atomic flip), -5 (redacted Debug + canary tracing test), -6 (no DB handle in crypto/transports public API) — all enforced by code or tests.

---

## 2. Frozen public contracts

These are **nailed down**. M3 must consume them, not redesign them.

### 2.1 `prro::crypto::CryptoProvider` trait

Authoritative source: `rust/prro/src/crypto/provider.rs`. The
signatures below are the frozen contract M3 binds to:

```rust
// Returned by sign_cms_detached.
pub struct SignedCmsBytes(pub Vec<u8>);
// Returned by fetch_cert_by_ski.
pub struct CertDer(pub Vec<u8>);
// Returned by verify_dstu (wrap-around-bool, not raw bool).
pub struct DstuVerifyResult(pub bool);

pub struct SignCmsRequest<'a> {
    pub session: &'a SigningSession,
    pub canonical_xml: &'a [u8],
    pub profile: prro_crypto::cms::profile::CmsProfile,
}

#[async_trait]
pub trait CryptoProvider: Send + Sync {
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError>;

    async fn verify_dstu(
        &self,
        content_digest: &[u8],
        sig_bytes: &[u8],          // 64-byte raw r||s LE-packed
        pubkey_compressed: &[u8],  // 33-byte LE compressed point
    ) -> Result<DstuVerifyResult, CryptoError>;

    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        originator_cert_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError>;

    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],            // owned strings; loaded from ca_endpoints
        ski: &[u8; 32],
        request_timeout: std::time::Duration,
    ) -> Result<CertDer, CryptoError>;
}
```

**Invariants:**
- No `SqlitePool`/`SqliteConnection`/`Pool`/`Transaction` in any public method signature (enforced by W5 static check at `rust/prro/tests/api_surface_no_db_handle.rs`).
- All errors typed via `CryptoError` enum with reason kinds (`SealKind` / `SignKind` / `DecryptKind` / `FetchKind` / `VerifyKind`) — no `String` errors, no `anyhow` downcast at call site.
- `SigningSession` + `SealedMaterial` use a manual redacted `Debug` impl; plaintext private-key bytes live in `Zeroizing<[u8; 32]>`. M3-introduced secret-bearing types MUST follow the same pattern (manual `Debug` + `Zeroizing<T>` for plaintext) — enforced by W6 canary test.
- Implementations choose their own threading model. `InProcessProvider` delegates sign / decrypt / cmp-fetch via `tokio::task::spawn_blocking`; verify stays on the executor. Future remote-sidecar provider is free to use HTTP — call sites must NOT assume `spawn_blocking` semantics.
- Return-type wrappers (`SignedCmsBytes`, `CertDer`, `DstuVerifyResult`) are deliberately newtype-around-primitive — call sites must accept them as opaque, not `.0`-unwrap into bare `Vec<u8>` / `bool` and lose the type-tag.

**Adding a method requires:** ADR amendment + W5 test fixture update + W6 needle review.

### 2.2 `prro::transports::dps::DpsChannel` trait

Authoritative source: `rust/prro/src/transports/dps/channel.rs`.
The block below is an abbreviated method inventory; DTO details live
in `rust/prro/src/transports/dps/dto.rs`.

```rust
#[async_trait]
pub trait DpsChannel: Send + Sync {
    async fn send_chk(...) -> Result<..., DpsError>;
    async fn last_chk(...) -> Result<..., DpsError>;
    async fn ping(...) -> Result<..., DpsError>;
    async fn status_rro(...) -> Result<..., DpsError>;
    async fn info_rro(...) -> Result<..., DpsError>;
    async fn query_by_local_identity(...) -> Result<..., DpsError>;
    // Default impl below — DO NOT override at call site.
    async fn by_server_fiscal_no(...) -> Result<..., DpsError> {
        // = last_chk(fn_sign) + response.id match (PRRO_GATE-5js)
    }
}
```

**Invariants:**
- Same no-DB-handle rule as CryptoProvider (enforced by W5).
- `grpc-timeout` HTTP/2 metadata MUST be set per RPC via `tonic::Request::set_timeout` (enforced by W3-C4 test `grpc_timeout_metadata_set_on_every_rpc`).
- `DpsError::ServerFiscalIdMismatch` is the **only** way to surface a fn_sign vs response.id divergence — call sites must NOT silently coerce.
- `DpsError::QueryNotSupported` reserved for transports without `query_by_local_identity`.
- Channel reuse is required (don't reconnect per RPC); see `GrpcDpsChannel::connect`.

**Adding a method requires:** updating vendored `proto/fiscal_server.proto`, regenerating, updating mock server in test harness.

### 2.3 XML goldens (ADR-M2-3)

**Oracle = Python `src/prro_gateway/serializers/dps_xml.py`. Rust = candidate.**

Frozen artefacts under `rust/prro/tests/goldens/`:
- `xml/shift_open.bin`, `xml/sell.bin`, `xml/return.bin`, `xml/z_report.bin`
- `cms/deterministic_prefix.bin` (== `xml/sell.bin` byte-for-byte)
- `prevhash/seed.bin` (32 zero bytes)

**Invariants:**
- Byte-equality REQUIRED for canonical XML — full-overlap scan, no length-only verdict shortcut.
- Re-capture is a **deliberate-spec-change action** — operator runs `regenerate.py` manually with reviewer checklist (`docs/M2-goldens-capture.md`); CI never auto-regenerates.
- Missing fixture = hard test failure with remediation message; **no skip-if-missing**.
- WebCheck CloseShift IS DPS Z_REPORT — no separate `xml/shift_close.bin`. M3 adapter must map this at boundary (PRRO_GATE-zti).
- KVT1/KVT2 byte-equivalence is **deferred** to a follow-up round once M3 has KVT parsers; the absence of those fixtures is intentional.

**Changing a golden requires:** explicit operator capture run + reviewer checklist sign-off + rationale in commit message.

### 2.4 ADR gates (W5 + W6)

These are **CI-enforced** — not policy text:

- **W5 (`api_surface_no_db_handle.rs`):** `crypto::` and `transports::` public APIs MUST NOT mention `SqlitePool`, `SqliteConnection`, `Pool`, `Transaction` — including via `impl Trait`, `dyn Trait`, function pointer params, or associated types. `services/` is exempt (carve-out for `cert_refresher`).
- **W6 (`secret_flow_tracing.rs`):** No seeded password/salt/private-key substring may appear in any captured `tracing` event or in `Debug`/`Display` of `CryptoError`/`SigningSession`/`SealedMaterial`, in any of 6 encoding forms.

**Adding `services/` integration that needs `crypto::` access:** must call through the trait; cannot inline-reach into `prro_crypto` and bypass redaction.

**Adding new secret-bearing type:** add seeded fixture to W6 needles + verify Debug redacts + add zeroize.

---

## 3. Known defers for pilot

Filed as `bd` issues, child-of M2 epic (PRRO_GATE-82j) where applicable. Each is **out-of-scope for M3 unless explicitly promoted** by user decision; M3 plan must NOT silently absorb these.

| bd | Pri | Title | Pilot impact |
|---|---|---|---|
| PRRO_GATE-0ps | P1 | DPS proto drift vs WebCheck TaxGrpc decompilation | Methods/fields beyond M2's vendored 6 may be needed in pilot field cycles. |
| PRRO_GATE-k54 | P1 | DPS TLS CA bundle support for `GrpcDpsChannel` | M2 uses tonic default trust store; production DPS needs explicit CA pinning. |
| PRRO_GATE-6bj | P1 | M3 write-path: WebCheck-derived submit retry/recovery policy | Concrete retry/backoff/dead-letter rules; must land in M3 design, not implementation. |
| PRRO_GATE-zti | P1 | M3 ingress: WebCheck CloseShift maps to DPS Z_REPORT | Adapter rename or map-at-boundary. Canonical issue (PRRO_GATE-js6 closed as duplicate 2026-05-06). |
| PRRO_GATE-gx2 | P1 | Pilot decision: offline lifecycle parity (offline pool + sync) | Not in M3 write-path scope; pilot-gating decision. |
| PRRO_GATE-iap | P2 | Pilot decision: WebCheck COM/1C 19-method compatibility | 1C interop bridge. |
| PRRO_GATE-3a8 | P2 | Pilot decision: print/export/check URL parity | Receipt print + tax-portal URL forms. |
| PRRO_GATE-ddn | P1 | M3 write-path: enforce `lnd` monotonicity source-of-truth | UNIQUE constraint or generator; M3 must pick one and pin. |
| PRRO_GATE-k99 | P1 | M3 write-path: ensure `transition_state` called under `with_immediate` | Or harden the helper to require it. |
| PRRO_GATE-ah8 | P1 | T14 `App::boot` must NOT blindly upsert_initial existing FN rows | Mask shift_state on existing rows during boot. |
| PRRO_GATE-1n9 | P2 | Type-safe wrappers in `IngressInboxRepo` (RequestId, InboxStatus) | M3 polishing; not pre-req. |
| PRRO_GATE-6r7 | P2 | Concurrent race test for `IngressInboxRepo` idempotency | Test debt; not pre-req. |
| PRRO_GATE-u8z | P3 | Workspace hardening: move non-root Cargo profiles to workspace root | Build hygiene. |
| PRRO_GATE-er6 | P0 | Sprint 2 step 1: OfflineSyncService selector for OFFLINE_LOCAL_ACK | Python-side; M3 entry should confirm this is decoupled from Rust write-path. |

**Prep before M3 plan:** triage P1s (especially PRRO_GATE-ddn, -k99, -ah8) — these are M3 entry constraints, NOT defers. They're listed here because they were discovered during M2 but their resolution lives in M3.

---

## 4. M3 entry constraints (frozen invariants the M3 plan inherits)

These come from the project CLAUDE.md "Frozen invariants" + M2 discoveries.

### 4.1 Inviolable invariants

- **No network or crypto inside long SQLite write transactions.** Compute fingerprints, fetch certs, build CMS bytes, talk to DPS — all OUTSIDE `with_immediate`. M2 cert_refresher proves the pattern (`compute_fingerprint` hoisted above tx; key-roll splits stage from flip via `INSERT … ON CONFLICT(ski_hex) DO UPDATE`).
- **One `fiscal_number` = one logical single-writer write-path.** M3 must respect lease model.
- **Channel switch forbidden with an open shift.** State-machine guard precedes any backend route swap.
- **Idempotency mandatory.** Inbox `request_id` unique; replays must be no-ops, not partial executions.
- **Offline must respect time + code limits.**
- **Adapters must build full canonical payloads, not summary-only payloads.**
- **All canonical envelopes must carry `schema_version`.**
- **Recovery and reconciliation must not silently violate state transitions.**
- **Graceful shutdown matters more than "finishing fast".**
- **Checkbox-compatible local-signing bypass must be config/profile-driven, not accidental drift.**

### 4.2 M3 must source from M2 (not reinvent)

- **Crypto path:** `prro::crypto::InProcessProvider` for sign + envelope unwrap + verify. A sidecar provider is future scope; the trait already accommodates it.
- **CA refresh:** `services::cert_refresher::refresh_for_fn` returns `Result<RefreshOutcome, RefreshError>`. M3 worker MUST treat `RefreshError` as terminal for that fn until next scheduled retry — do NOT fall back to "use whatever is in DB". `Failed-as-Ok` was deliberately dropped from the contract.
- **DPS transport:** `prro::transports::dps::GrpcDpsChannel` — never construct per-RPC; reuse channel; set per-call timeout via the trait's `request<T>` helper.
- **Canonical XML:** `prro::xml::build_canonical_xml(&CanonicalDoc) -> Result<Vec<u8>>` for the 4 doc types. Bytes are fixed; **do not edit the builder without re-capturing goldens** with operator + reviewer.
- **Goldens harness:** any new doc type added in M3 must ship with a `regenerate.py` capture entry + a `tests/goldens_byte_equiv.rs` test before merge.

### 4.3 M3 must pick (open M3-entry decisions)

- **`lnd` monotonicity source:** UNIQUE constraint vs generator function (PRRO_GATE-ddn). Decision goes in M3 plan.
- **`transition_state` lock discipline:** call-site requirement vs helper-side hardening (PRRO_GATE-k99). Decision goes in M3 plan.
- **`App::boot` upsert behaviour:** preserve shift_state on existing FN rows (PRRO_GATE-ah8). Must be fixed before any FN row is touched in M3.
- **Retry/recovery policy shape:** dead-letter, max attempts, backoff curve (PRRO_GATE-6bj). Must be designed in M3 plan, not improvised in implementation tasks.
- **CloseShift adapter mapping:** rename Python adapter or map-at-boundary (PRRO_GATE-zti). Edge of M2/M3; decide before write-path consumes it.

### 4.4 M3 must explicitly defer

Anything in §3 not listed in §4.3. M3 plan should name each as out-of-scope and link the bd, mirroring how M2 plan named M3 write-path internals as out-of-scope.

---

## 5. Branch / git state at handoff

- **Branch:** `rust-gateway`. **M2 code baseline: `66e317c`** on `origin/rust-gateway` — this is the SHA M3 entry constraints in §4 are anchored to. Subsequent commits (including this handoff document) advance the branch HEAD; M3 plan should re-anchor against the M3-entry SHA at the moment M3 starts.
- **Behind main:** check `git log main..rust-gateway` at integration time; do not merge to main without an integration plan.
- **Pending pushes:** none (everything M2 is on `origin/rust-gateway`).
- **Test gates green** as of `66e317c`: 164 passing.

M3 plan should be drafted in a new file under `docs/superpowers/plans/` and reference this handoff in its inputs section.
