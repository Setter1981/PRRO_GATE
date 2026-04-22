# Rust Review - 2026-04-22

Scope: Rust workspace under `rust/`, with special attention to fiscal write path,
Maria304 bridge, sidecar, crypto, printer daemon, operational configs, and PRRO
invariants.

Reviewer stance: senior Rust / fintech systems review. Findings are prioritized by
production risk, not by ease of fixing.

## Baseline Review

Verdict: the Rust workspace has improved substantially and the full test suite is
green, but I would not pass it as a production gate yet. The largest remaining
risks are idempotency, authentication boundaries, ambiguous retry outcomes, and
operational configuration drift.

### Critical / High

1. `prro_sidecar` exposes `POST /fiscal/send` without authentication.
   The route is attached directly in `rust/prro_sidecar/src/bin/prro_sidecar.rs`
   and the handler accepts only `Json<CanonicalCommand>`. If the service is
   exposed beyond loopback, any client can trigger fiscal signing and DPS
   submission with the active key.

   Required fix: add bearer auth, mTLS, Unix socket binding, or fail startup when
   bound to a non-loopback address without auth.

2. `prro_sidecar` ignores `CanonicalCommand.idempotency_key`.
   The field exists in `rust/prro_sidecar/src/input.rs`, but the sidecar write
   path does not store or replay by it. Each retry allocates a fresh local number
   and can submit the same business operation again.

   Required fix: add a durable sidecar request journal keyed by
   `idempotency_key + payload_sha256`, with stored response replay and rejection
   for the same key with a different payload hash.

3. The outer request timeout can create an ambiguous DPS outcome.
   `handle_fiscal_send` wraps the full sidecar flow in a wall-clock timeout. If
   the timeout fires after the request reaches DPS but before the response and
   MAC hash are persisted, a retry can create a duplicate fiscal submission.

   Required fix: persist an `IN_FLIGHT` / `SENT` / `ACK` / `UNKNOWN` state before
   network submission, set per-RPC deadlines, and reconcile ambiguous outcomes
   before allowing another document for the same fiscal number.

4. Maria304 idempotency is session-local.
   The Maria304 driver builds idempotency keys from `fiscal_number`,
   `session_uuid`, and `receipt_seq`. A reconnect after an accepted operation but
   before the client receives the response can generate a new key for the same
   fiscal operation.

   Required fix: use a stable device-side operation identity, durable pending
   receipt state, or content/business-field deduplication across reconnects.

5. Maria304 admin auth can be disabled silently with an empty token.
   The admin API accepts every request when the configured token is empty. In
   live mode, this should be a startup error unless explicitly running in a
   loopback/dev profile.

6. Maria304 config examples use `${ENV_VAR}` placeholders, but the binary parses
   YAML directly without environment substitution.
   This can leave literal bearer tokens such as `${MARIA304_BRIDGE_TOKEN}` in
   live configuration.

### Medium

7. Demo license limits are computed but not enforced in the sidecar write path.
   `LicenseState::Demo { limits }` is accepted the same way as valid/grace
   licenses.

8. `rust/prro_sidecar/ops/sidecar.example.toml` does not match the actual config
   schema. It documents `license.path`, DPS timeout fields, `skip_dps`, and
   `DEBUG_INSECURE_MODE`, while the current config expects license payload and
   signature fields, has no DPS timeout fields there, and gates `skip_sign` on
   `DEV_MODE`.

9. `prro_escpos_daemon` exposes unauthenticated `/print` where the request body
   chooses a TCP destination. Default loopback is good, but a non-loopback
   deployment becomes an internal raw TCP write primitive.

10. The DPS gRPC client lacks explicit per-RPC deadlines. The outer HTTP timeout
    is not a substitute because it contributes to ambiguous cancellation.

11. `prro_crypto_v2` is in the workspace but appears unused by other crates.
    It should either be wired as a deliberate replacement path with differential
    tests against v1 or kept out of the production workspace to avoid crypto
    implementation drift.

### Positive Observations

- Maria304 driver has stronger protocol handling than before: password parsing,
  CRC behavior, connection gate/cooldown, CP866 coverage, constant-time bearer
  comparison when a token is configured, and one-connection-per-FN behavior.
- Sidecar has important safety improvements: per-FN serialization around the
  MAC chain, fail-closed certificate validity checks, TSP off the async worker
  path, Kyiv epoch alignment with Python, accepted-only MAC persistence, and a
  degraded-state guard.
- Crypto tests and benches are broad and currently pass.

### Verification

Command used:

```text
cmd.exe /c "cd /d D:\prro_gate\rust && cargo test --workspace --all-targets"
```

Result: passed across the Rust workspace using Windows `cargo 1.95.0`. WSL cargo
was not available in the shell environment.

## Second Slow Pass

Additional focus: issues not covered by the baseline pass, especially config
uniqueness, shutdown correctness, certificate binding, parser/builder edge cases,
and trust boundaries inside the Rust crypto/TSP path.

### Critical / High

1. Maria304 allows duplicate `fiscal_number` listeners in one process.
   `maria304_driver` iterates `cfg.listeners` and creates a fresh `FnListener`
   for each entry. Each listener owns its own `ConnectionGate`, so two config
   entries with the same `fiscal_number` but different TCP ports bypass the
   intended "single connection per FN" invariant. `Registry::register_fn` just
   appends entries and does not enforce uniqueness.

   Impact: two 1C instances can operate concurrently against the same fiscal
   number, with separate session UUIDs and receipt sequences. This can produce
   duplicate or out-of-order fiscal submissions.

   Required fix: validate config before spawning listeners. Reject duplicate
   `fiscal_number` and duplicate bind addresses. Ideally make the gate keyed by
   fiscal number at registry level, not only per listener instance.

2. Maria304 graceful shutdown is parsed but not implemented.
   `deployment.graceful_shutdown_timeout_s` is parsed, but after `ctrl_c()` the
   binary logs and returns immediately. Listener and admin tasks are detached
   `tokio::spawn` handles, with no cancellation token, no listener drain, and no
   wait for in-flight bridge submissions.

   Impact: a Ctrl-C or service stop can abort an in-flight COMP submission while
   the client state is ambiguous. This is the same class of fiscal retry risk as
   the sidecar timeout issue, but on the Maria304 ingress side.

   Required fix: keep task handles, stop accepting new TCP sessions, let the
   active session finish or timeout, then shut down within the configured grace
   period.

3. Sidecar validates DB certificate metadata but may sign with a different cert
   embedded in the key container.
   The hot path checks `operator_certs.valid_from/valid_to` for the requested FN,
   then extracts the JKS/container key. If the container contains a certificate,
   that embedded cert is used for CMS. There is no fingerprint/SKI comparison
   between the embedded cert and the DB metadata row, and no second validity
   check against the actual cert selected for signing.

   Impact: stale DB metadata can authorize signing with an expired, replaced, or
   wrong embedded cert. In a fintech signing path, the metadata row and signing
   certificate must be cryptographically bound.

   Required fix: after selecting `cert_der`, compute fingerprint/SKI and compare
   to `operator_certs`; extract validity from the selected cert and fail closed
   on mismatch or expiry.

4. CAdES-T/TSP trust model is weaker than the sidecar path implies.
   `sign_with_tsp` fetches a TSP response and embeds the returned
   `TimeStampToken`. The crate comments explicitly say the TSP token is returned
   "as delivered" and TSA signature/chain verification is not performed. If the
   configured TSA URL is plain HTTP or TLS trust is compromised, an attacker can
   substitute a timestamp token.

   Impact: this may still be caught by DPS, but the local sidecar treats the CMS
   as successfully timestamped before any local verification. This is not strong
   enough for a legal/financial timestamp trust boundary.

   Required fix: verify the TST signature, message imprint, TSA certificate
   purpose, and chain, or explicitly mark TSP as transport-trusted only and keep
   it out of any local legal/audit claim.

### Medium

5. `COMP` response builder allows over-width numeric segments after accepted
   fiscal response.
   `CompBuilder::to_body` uses `"{s:010}"`, which pads but does not truncate.
   The code intentionally allows values longer than 10 digits as a diagnostic
   signal, but `classify_response` accepts any positive `u64` fiscal ID. The
   dispatcher then closes the receipt and sends a malformed `COMP` payload to
   1C.

   Required fix: enforce `<= 9_999_999_999` for fiscal ID and every COMP numeric
   segment before returning `DocumentOutcome::Accepted`. Over-width response is
   a gateway contract violation and should not close the local receipt as a
   clean success.

6. Sidecar XML builder signs fiscally invalid numeric shapes instead of
   rejecting them before local number allocation.
   The input model and XML builder accept negative/zero payments, missing sums
   defaulted to zero, and arbitrary item/payment totals. Some tests document
   this as "domain validation downstream". But in sidecar mode, downstream DPS
   rejection happens after `next_local_number` has already advanced.

   Required fix: validate receipt economics before local number allocation:
   non-negative amounts where required, positive quantities, item sums matching
   price/quantity/discounts within rounding policy, payments matching totals,
   and service/cash withdrawal sums within business limits.

7. Runtime timeout knobs are not bounded.
   Maria304 bridge timeouts, retry attempts, listener idle timeout, cooldown, and
   ESC/POS `/print.timeout_ms` are accepted as raw integers. A bad config or
   request can create immediate timeouts, very long hangs, or excessive retry
   latency.

   Required fix: add validation ranges and reject unreasonable values at config
   parse time. For request-level printer timeout, clamp or reject per API.

### Tests / Verification Notes

- The first full Rust workspace test pass succeeded.
- The second full Rust workspace test pass also succeeded:

```text
cmd.exe /c "cd /d D:\prro_gate\rust && cargo test --workspace --all-targets"
```

Result: passed. Notable warnings remain: ignored non-root package profiles,
unused imports, and substantial dead-code warnings in `prro_crypto_v2`.

## Third Slow Pass

Additional focus: production blockers hidden by green tests: embedded key
material, unused status classifiers, report-state handling, readiness semantics,
canonical envelope integrity, and observability wiring.

### Critical / High

1. Embedded sidecar license public keys are placeholders with the wrong length.
   `license.rs` includes `license_pubkey_current.der` and
   `license_pubkey_next.der`, and `verify_detached()` requires a 33-byte
   compressed DSTU PB-257 public key. Both embedded files are 32 bytes and are
   filled with `DE AD BE EF` repeated.

   Impact: public `verify_signature_only()` and per-request `verify()` use only
   these embedded keys. Any real license signed by the generated 33-byte public
   key will verify as `SignatureInvalid`; `prro_admin load-license` will reject
   it, and an already-installed license will fail in the sidecar write path.
   Existing tests inject generated test pubkeys, so this production key-path is
   not covered.

   Required fix: replace both embedded files with real 33-byte compressed public
   keys from `prro_license_keygen`, add a unit test asserting embedded key
   lengths are 33 and not the placeholder value, and add an end-to-end license
   fixture signed by the current embedded key.

2. Sidecar DPS status classification is implemented but unused in the write path.
   `grpc_client::classify_dps_status()` classifies `-3`, `-4`, and `-12` as
   transient and other named codes as permanent. `fiscal_send_inner()` logs the
   response status but then returns HTTP 200 with the raw negative status and no
   state transition.

   Impact: `ERROR_BAD_HASH_PREV (-12)` does not put the FN into degraded/resync
   mode; `ERROR_SAVE (-3)` and `ERROR_UNKNOWN (-4)` are not handled as ambiguous
   retryable outcomes; permanent XML/signature errors still consume local numbers
   after the sidecar has already signed and sent. The classifier tests are green,
   but the classifier is dead for the actual fiscal send.

   Required fix: invoke `classify_dps_status()` immediately after the DPS
   response. For transient/chain errors, persist a request status and degraded or
   resync marker before returning. For permanent errors, return an explicit
   non-success HTTP outcome that the Python caller cannot confuse with accepted
   delivery.

3. Maria304 report submission ignores `CanonicalResponse.ok`.
   Receipts use `classify_response()` before closing a receipt, but
   `submit_report()` treats any `Ok(CanonicalResponse)` from the bridge as
   success. A Python gateway response with HTTP 200 and `ok=false` /
   `document_state=ERROR_*` will still produce `DONE + READY` and increment
   `receipt_seq`.

   Impact: Z-report, X-report, shift-open, periodic reports, and related report
   commands can be acknowledged to 1C as successful even when the gateway says
   the fiscal operation failed. For Z-report this can corrupt fiscal-day
   operator state.

   Required fix: classify report responses with the same rigor as COMP. At
   minimum, require `resp.ok == true` and a report-appropriate accepted state
   before `DONE + READY`; otherwise map to terminal/retryable SOFT code without
   advancing correlation.

4. Sidecar readiness reports OK for invalid or expired licenses.
   `/health/ready` only checks that an active license row exists. It does not
   verify signature, expiry, TIN, FN coverage, or even the embedded public key
   path.

   Impact: orchestration can mark the sidecar ready even though every fiscal send
   will fail license validation. This is especially dangerous with the embedded
   placeholder pubkey issue above.

   Required fix: readiness should run at least signature+expiry validation of the
   active license, and preferably validate configured FNs against license scope.

5. Canonical envelope integrity fields are not validated by sidecar.
   `schema_version`, `request_id`, `idempotency_key`, and `payload_sha256` are
   deserialized, but `fiscal_send_inner()` only validates `operation_type`.
   `schema_version` can be any string, and `payload_sha256` is never recomputed
   against `payload`.

   Impact: Rust can sign a payload under a spoofed hash or future/unknown schema.
   This undermines auditability and makes the future idempotency journal unsafe
   if it trusts caller-provided hashes.

   Required fix: reject unsupported `schema_version`, recompute SHA-256 over the
   canonical payload form expected by Python, and reject mismatch before local
   number allocation.

### Medium

6. Maria304 admin metrics are wired to fresh counters that the listener never
   updates.
   `maria304_driver` creates `SessionMetrics` and registers it in the admin
   registry, but no metrics object is passed into `FnListener`, `run_connection`,
   or `dispatch`. The `record_*` methods are only used in unit tests.

   Impact: `/admin/metrics` can stay at zero during live traffic, which removes
   an important operator signal for frame errors, bridge failures, and receipt
   ACK rates.

   Required fix: pass the per-FN metrics handle into listener/session code and
   increment counters at decode, write, COMP accepted, CANC, bridge error, and
   frame error points.

7. Maria304 runs blocking HTTP and blocking retry sleeps inside Tokio worker
   tasks.
   `HttpBridge` uses `reqwest::blocking`, `RetryBridge` uses a blocking sleeper,
   and `dispatch()` calls `bridge.submit()` synchronously from the async
   connection loop.

   Impact: with several configured FNs, slow gateway calls or retries can occupy
   Tokio worker threads and delay unrelated listeners/admin handling. The current
   single-connection-per-FN design limits per-FN concurrency, but not process-wide
   worker starvation.

   Required fix: move bridge submissions into `tokio::task::spawn_blocking`, or
   switch the bridge trait to async and use async reqwest/tokio sleep.

### Tests / Verification Notes

- This pass found failures that current tests do not exercise:
  embedded production license pubkeys, sidecar handling of negative DPS statuses,
  report responses with `ok=false`, readiness with invalid license, and live
  metrics increments.
- Recommended new tests:
  embedded pubkey length/non-placeholder test; sidecar fake-DPS negative status
  tests for `-3`, `-4`, `-12`, and permanent XML/signature codes; report
  `CanonicalResponse { ok: false }` tests; readiness test with invalid license;
  admin metrics integration test over a real TCP session.
- Third full Rust workspace test pass:

```text
cmd.exe /c "cd /d D:\prro_gate\rust && cargo test --workspace --all-targets"
```

Result: passed. The green suite does not cover the production embedded license
pubkeys or the negative-DPS/report-failure scenarios listed above.

## Fourth Pass / Recheck And Triage

Date: 2026-04-22.
Scope: targeted re-read of the previous high-risk claims, with emphasis on
discarding weak findings and separating confirmed defects from design-dependent
risks.

### Confirmed Blockers

1. The embedded production license public keys are unusable.
   Recheck result: confirmed, not speculative.

   Evidence: `license.rs` includes `license_pubkey_current.der` and
   `license_pubkey_next.der`, while `verify_signature_with_pubkey()` rejects any
   public key that is not exactly 33 bytes. Both embedded files are 32 bytes and
   contain the `DE AD BE EF` placeholder pattern.

   Why this matters: this is not a policy disagreement or a missing hardening
   step; it is a direct production-path incompatibility. A real Ed25519 compact
   signature is 64 bytes, but the public key path here expects a 33-byte
   compressed secp256k1 key. The shipped embedded key material cannot satisfy
   that check. The test suite passes because it injects generated test public
   keys instead of validating the embedded production files.

   Severity: blocker for any environment that depends on Rust sidecar license
   enforcement.

2. Sidecar still treats negative DPS statuses as successful HTTP responses.
   Recheck result: confirmed, high confidence.

   Evidence: `classify_dps_status()` exists and explicitly distinguishes
   transient `-3`, `-4`, `-12` from permanent negative statuses. The fiscal send
   handler calls `grpc_pool.send_chk_v2()`, logs `dps_status`, then returns
   `FiscalSendResponse { status: resp.status, ... }`. The classifier is only
   referenced by tests.

   Why this matters: the code has the correct domain vocabulary but not the
   production decision point. `ERROR_BAD_HASH_PREV (-12)` should trigger degraded
   or resync behavior before more checks are signed. `ERROR_SAVE (-3)` and
   `ERROR_UNKNOWN (-4)` are ambiguous/transient outcomes and should not look like
   a normal accepted send to Python. Permanent signing/XML errors should be
   surfaced as explicit failure, not a raw negative status inside HTTP 200.

   Severity: blocker for fiscal correctness and recovery semantics.

3. Sidecar accepts the canonical envelope's integrity fields without enforcing
   them.
   Recheck result: confirmed.

   Evidence: `CanonicalCommand` requires `schema_version`, `idempotency_key`,
   and `payload_sha256` at deserialization time. In the write path,
   `fiscal_send_inner()` validates `operation_type`, then proceeds to allocate a
   local number and build/sign/send XML. There is no version allow-list and no
   recomputation of `payload_sha256` over `payload`.

   Why this matters: deserialization is not integrity validation. The sidecar can
   sign and send a document where the caller-provided hash does not describe the
   actual payload, and can accept an unknown schema string. This weakens audit
   replay and makes a future durable idempotency journal unsafe if it trusts
   these fields.

   Severity: high; promote to blocker if `payload_sha256` is part of an external
   audit or reconciliation contract.

4. Sidecar request idempotency is not implemented in the Rust write path.
   Recheck result: confirmed.

   Evidence: `idempotency_key` is present in the input DTO, but the fiscal send
   path allocates a new `local_number` before signing and sending without using
   the key to look up, lock, persist, or replay an existing request. Search shows
   idempotency usage only in DTO/tests/other adapters, not in the handler.

   Why this matters: retry after timeout or client disconnect can create a second
   signed local document instead of replaying the prior outcome. This combines
   badly with the outer HTTP timeout and negative-DPS-status behavior.

   Severity: blocker for exactly-once fiscal send semantics.

5. Maria304 report-style commands ignore `CanonicalResponse.ok`.
   Recheck result: confirmed.

   Evidence: `submit_report()` constructs a canonical command, calls
   `bridge.submit(&envelope)`, and treats any `Ok(_)` as success:
   `mark_command_ok`, increment `receipt_seq`, return `DONE + READY`. The COMP
   path correctly calls `classify_response(&resp)`, but report submission does
   not.

   Why this matters: if Python returns HTTP 200 with `ok=false` and
   `document_state=ERROR_*`, Maria304 will acknowledge the report command as
   complete to 1C. This is especially dangerous for Z-report and shift state,
   because the driver can advance operator-visible fiscal state after a failed
   fiscal operation.

   Severity: blocker for report/Z-report correctness.

### Confirmed High-Risk Defects

6. Maria304 allows duplicate configured listeners for the same fiscal number.
   Recheck result: confirmed.

   Evidence: startup iterates over `cfg.listeners`, builds a fresh `FnListener`
   and a fresh `ConnectionGate` for each entry, then `Registry::register_fn()`
   pushes the entry into a vector. The single-connection exclusion gate is local
   to one listener instance; there is no startup uniqueness check by
   `fiscal_number`.

   Why this matters: the code comment correctly states the invariant: one
   simultaneous connection per FN. Duplicate config entries for the same FN break
   that invariant by creating independent listeners with independent gates.

   Severity: high. If config generation is fully controlled and validated
   elsewhere, impact is lower, but the binary itself currently does not enforce
   the invariant it relies on.

7. Maria304 graceful shutdown setting is parsed but not implemented.
   Recheck result: confirmed.

   Evidence: `DeploymentCfg.graceful_shutdown_timeout_s` exists with a default,
   but startup detaches listener/admin tasks with `tokio::spawn()`, awaits
   `ctrl_c()`, logs shutdown, and returns. No cancellation token, no listener
   drain, no connection drain, no timeout use.

   Why this matters: a stop signal can interrupt active TCP sessions or bridge
   submissions without a controlled response to 1C. The config field creates an
   operator expectation that the binary does not satisfy.

   Severity: high for production operation; medium if the service is not yet run
   under automated restarts.

8. Maria304 admin metrics are mostly decorative in runtime.
   Recheck result: confirmed.

   Evidence: `SessionMetrics` is created and registered, but it is not passed
   into `FnListener`, connection handling, frame dispatch, or bridge submission.
   `record_*` methods appear in unit tests and test helpers, not in the live
   path.

   Why this matters: operators can read `/admin/metrics` and see zeros while
   traffic and errors are happening. This is not a fiscal-data corruption bug,
   but it undermines production observability exactly where frame and bridge
   errors matter.

   Severity: medium/high operational risk.

9. Maria304 uses blocking bridge calls inside async connection tasks.
   Recheck result: confirmed, with nuance.

   Evidence: the bridge abstraction is synchronous, `HttpBridge` uses blocking
   HTTP, retry sleeping is blocking, and `dispatch()` invokes `bridge.submit()`
   directly from the async listener path.

   Why this matters: with a small number of FNs this may be tolerable, because
   each FN is single-connection gated. With several FNs or slow gateway retries,
   blocking work can occupy Tokio worker threads and delay unrelated listeners or
   admin handling.

   Severity: medium. Upgrade to high if this process is expected to host many
   FNs or operate across unreliable network links.

### Nuanced / Needs External Confirmation

10. ESC/POS `/print` exposure remains environment-dependent.
    Recheck result: valid risk, not always a blocker.

    If the printer adapter is bound only to loopback and protected by host
    controls, this is a medium local-hardening issue. If it can be reached from a
    LAN or another container without authentication, it becomes high: arbitrary
    jobs can be injected and printer state can be manipulated.

11. CAdES-T/TSP local verification depends on the legal trust boundary.
    Recheck result: keep as high-risk review item, but do not call it a proven
    code bug without DPS/legal confirmation.

    The sidecar obtains and embeds a timestamp token but does not locally build a
    TSA trust chain or verify the token signature. If DPS is the sole legal
    verifier by design, this can be an accepted trust split. If the sidecar is
    responsible for producing locally auditable CAdES-T evidence, this is a
    serious gap.

12. Certificate metadata validation versus actual signing certificate still
    needs an integration fixture.
    Recheck result: likely real design gap, but exact severity depends on the
    native signing backend.

    The Rust config/repository checks validate configured certificate metadata,
    while the actual private-key/cert chosen by the native signing library is not
    visibly cross-checked against that configured fingerprint/SKI in the reviewed
    path. A targeted integration test with a mismatched configured cert and JKS
    signing cert should decide whether this is blocker or high.

### Items Downgraded Or Clarified

13. The COMP width issue is a protocol-hardening defect, not a direct current
    corruption proof.

    `CompBuilder` intentionally allows >10-digit segments to produce a longer
    diagnostic body, and COMP tests may cover that behavior. For production,
    however, Maria304 wire format is fixed-width. The safer behavior is to reject
    an impossible fiscal id or total before composing a data frame, instead of
    emitting a malformed-length COMP frame after receipt closure. Keep as medium
    unless field evidence shows 1C misparses the overflowed body.

14. `/health/ready` is a readiness false-positive, not a write-path bypass.

    The write path still performs license validation later. The bug is that
    orchestration can declare the service ready when the active license row is
    expired, unsigned, scoped to another FN, or unverifiable due to the embedded
    key problem. Keep as high operational risk; its severity is amplified by the
    placeholder pubkey blocker.

### Recheck Conclusion

The strongest current Rust blockers are not style or theoretical concerns:

- embedded sidecar license keys cannot pass the verifier;
- sidecar does not enforce idempotency or payload hash/version integrity before
  local number allocation;
- sidecar negative DPS statuses bypass the existing classifier;
- Maria304 report commands acknowledge `ok=false` bridge responses as success;
- Maria304 can violate one-listener-per-FN if duplicate FNs appear in config.

The fourth pass did not invalidate the major findings. It did downgrade a few
items to environment-dependent risks (`/print`, TSP local verification,
certificate metadata binding) and medium protocol hardening (COMP overflow), but
the fiscal write-path issues remain severe.
