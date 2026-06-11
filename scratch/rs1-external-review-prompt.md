# External Code Review Request — RS-1 runtime-supervisor crypto seam (Multi-Protocol PRRO Gateway)

## Your role

Senior Rust engineer with fiscal-systems + applied-cryptography experience (DSTU-4145 / CMS / CAdES, keystore handling, secret-material hygiene). You are reviewing a feature branch of a Ukrainian **PRRO** (Programmable Registrar of Settlement Operations) gateway — a local edge service issuing fiscal receipts for retail/HoReCa, with offline resilience, multi-protocol ingress, and a DPS (State Tax Service) submission channel over DSTU crypto. **Correctness of fiscal/crypto behavior, auditability, and secret hygiene matter far more than performance or ergonomics.** Be terse; signal-to-noise over thoroughness theatre.

## How to retrieve the change

Public GitHub repo. The change is a **branch** (`feat/rs1-runtime-supervisor`), **not yet a PR** — base `a940520` (rust-gateway), head `062be05`. Crate: `rust/prro`; sibling crypto crate: `rust/prro_crypto`.

**Browse the full diff in your web view (no clone needed):**
- Diff (base..head): <https://github.com/Setter1981/PRRO_GATE/compare/a940520...feat/rs1-runtime-supervisor>
- Commits (5): <https://github.com/Setter1981/PRRO_GATE/commits/feat/rs1-runtime-supervisor>
- Key files on the branch (read these in full):
  - <https://github.com/Setter1981/PRRO_GATE/blob/feat/rs1-runtime-supervisor/rust/prro/src/runtime/key_loader.rs>
  - <https://github.com/Setter1981/PRRO_GATE/blob/feat/rs1-runtime-supervisor/rust/prro/src/crypto/session.rs>
  - <https://github.com/Setter1981/PRRO_GATE/blob/feat/rs1-runtime-supervisor/rust/prro/src/runtime/bindings.rs>
  - <https://github.com/Setter1981/PRRO_GATE/blob/feat/rs1-runtime-supervisor/rust/prro/src/config/mod.rs>
  - <https://github.com/Setter1981/PRRO_GATE/blob/feat/rs1-runtime-supervisor/rust/prro/tests/rs1_build_fn_sign.rs>
- Supporting (unchanged, but referenced by the diff): `rust/prro_crypto/src/interop/prro/containers.rs` (`ExtractedKey`, `signing_cert`), `rust/prro_crypto/src/cms/builder.rs` (`CmsSigner`/`sign_with`/`CmsBuildOptions`), `rust/prro/src/crypto/errors.rs` (`CryptoError` Debug), `rust/prro/src/crypto/session.rs` (`unseal_jks`, the model `from_extracted` mirrors).

**Or clone + diff locally:**
```bash
git clone https://github.com/Setter1981/PRRO_GATE && cd PRRO_GATE
git fetch origin rust-gateway feat/rs1-runtime-supervisor
git diff a940520..062be05            # base..head
git log --oneline a940520..062be05   # the 5 RS-1 commits
```

## What this branch is (RS-1, pieces 1–3b of the runtime spine)

The deployable binary (`prro serve`) today boots then **idles** — none of the built+tested write-path/reconcile/drain machinery is driven. RS-1 begins wiring the **runtime supervisor / composition-root** that will drive it. This branch lands the **config gating + the crypto deps construction** (the highest-care seam); the supervisor task itself + ingress + the live worker are later pieces (RS-1 Piece 5 + RS-2/3), **explicitly not in this branch**.

Changed files (`rust/prro`):
- **`src/config/mod.rs`** — Piece 1: `SupervisorCfg { enabled (default false), dps: DpsCfg { endpoint: Option<String>, request_timeout_seconds } }`. The supervisor is **gated off by default** (rollback seam: binary stays M1-idle until an explicit config flip). `dps.endpoint` is **fail-closed ONLY when `enabled = true`** (validated at supervisor startup via `require_dps_endpoint`, NOT at parse time) — a default-off binary must boot without an endpoint.
- **`src/crypto/session.rs`** — Piece 2: new production ctor `SigningSession::from_extracted(operator_id, ExtractedKey) -> Result<Self, CryptoError>`, alongside the existing `unseal_jks`. Selects the signing cert via `ExtractedKey::signing_cert()`; moves the `Zeroizing<[u8;32]>` scalar; stores `operator_id` verbatim. + 2 unit tests.
- **`src/runtime/key_loader.rs`** (NEW) — Piece 3: `JksOperatorKeyLoader` impl of the `OperatorKeyLoader` trait (reads JKS file + plaintext password → `extract_private_key` → `from_extracted` → `SigningContext`). Piece 3b: `build_fn_sign(session, fiscal_number) -> Result<CheckSignBlob, CmsError>` — native ATTACHED CAdES-BES CMS over the FN string.
- **`src/runtime/bindings.rs`** — the `OperatorKeyLoader::load` trait gained a first param `operator_id: &str`; `build_from_db` passes the authoritative `row.operator_id` from the secure `operators` table. (4 test-loader impls + a registry test updated.)
- **tests** — `tests/rs1_build_fn_sign.rs` (new), `tests/bindings_registry_build.rs` (operator_id-threading assert), 3 loader signature updates, `tests/common/mod.rs` helper.

## Domain facts you need (load-bearing)

- **The "-14 `CryptBadSign`" trap** (fixed 2026-05-29, live-confirmed against the ДПС verifier): a UA EDS JKS ships BOTH a `digitalSignature` cert AND a `keyAgreement` (encryption) cert + CA chain. Embedding `certs[0]` (frequently the encryption cert) makes DPS reject the signature. The fix is `ExtractedKey::signing_cert()` (`prro_crypto/src/interop/prro/containers.rs`), which selects `KeyUsage=digitalSignature` (falling back to `certs.first()`).
- **Secret-material discipline** (doc-block in `bindings.rs` ~119-140; audit Round-2 R2-4): the loader's `password: &[u8]` borrows a caller-owned `Zeroizing<Vec<u8>>` wiped on drop; impls MUST NOT clone it into un-zeroized heap; the returned `SigningContext` MUST NOT retain the plaintext password; secret-bearing types MUST NOT `#[derive(Debug)]` (manual redacted Debug; ADR-M2-5 §4d).
- **`operator_id` is the cashier's INN (ІПН) = PII** — must NOT reach process logs (journald/Loki); only `audit_log`.
- **`build_fn_sign` is a verbatim port** of the live-DPS-ЄВПЗ-accepted (2026-05-29) W4-Z3 `sign_fn_blob` (`attached: true` + `signing_time`, profile `Dstu4145WithGost34311Pb`, content = FN string bytes).
- **Hardening boundary (deliberate):** the JKS password is handled **FLAT (unsealed)** in `JksOperatorKeyLoader` — the loader calls `extract_private_key` directly (not the sealed `unseal_jks` path, which needs a `cred_salt` the `operators` table lacks). Sealing the JKS password at-rest is a **separately-tracked follow-up**, intentionally NOT in this branch. The docs say so explicitly — confirm the branch does not *pretend* it is solved.

## Frozen invariants (CLAUDE.md — all 10; the relevant ones)

1. **No network or crypto inside a long SQLite write transaction.** 4. **Idempotency mandatory.** 9. **Graceful shutdown matters more than finishing fast.** 10. **Local signing bypassed only by explicit config, not accidental drift.** (Walk each against the diff; confirm preserved or N/A.)

## What was already reviewed (focus on what this MIGHT have missed)

This branch passed an internal 2-lens independent review (crypto/secret-discipline + correctness/invariants); both said MERGE. Findings already **addressed**: (a) a PII leak risk — `from_extracted`'s error was mapped via `format!("{e:?}")` and `CryptoError::JksUnseal`'s Debug embeds `operator_id` → replaced with a fixed PII-free string (commit `2c25351`); (b) `build_fn_sign` had no test → added a structural CMS-well-formedness test over a real DSTU cert fixture (commit `062be05`). Tracked-not-done: `map_container_err`'s `format!("{other:?}")` (verified no `ContainerError` variant carries secret/PII today, and the string is never logged — optional per-variant labels for symmetry); a real-JKS integration test for `JksOperatorKeyLoader` (owed to Piece 5).

**Verify those addressals are correct + complete, and find what a fresh adversarial reviewer would catch that two prior passes did not.** Specifically hunt:

- **A — crypto correctness:** Is the -14 trap closed on *every* path (`from_extracted` AND `build_fn_sign`, where the latter signs with `session.cert_der()`)? Any CMS shape error (attached vs detached, profile, signed-attrs)? Is `FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words)` correct for a 32-byte LE scalar on PB-257?
- **B — secret leaks (adversarial):** Trace every `format!`/`Debug`/`tracing`/`println`/error string + the `KeyLoadFailure::Other(String)` payload, the audit payload, and the `SigningContext`/`SigningSession` for ANY path where `operator_id`, the password, or `param_d` could escape to a non-audit surface. Does the `&[u8]` → `&str` (`from_utf8`) → `Zeroizing<String>` conversion leave any transient plaintext un-zeroized? Is `param_d` ever copied (vs moved)?
- **C — API/trait change:** Is extending `OperatorKeyLoader::load` with `operator_id` the right design (vs deriving it from the cert subject)? Are all impls + callers updated? Does threading PII through the trait widen the leak surface?
- **D — config gating soundness:** Can a half-configured/`enabled=true`-but-misconfigured supervisor cause harm, or is fail-closed airtight? Is `enabled=false` truly behavior-identical to the prior M1-idle boot?
- **E — error semantics:** `from_extracted` reuses `CryptoError::JksUnseal{reason: KeyExtractionFailed}` for "no signing cert" (no unsealing happened) — acceptable or misleading? Any `.expect()`/`.unwrap()`/`panic!` on a **production** path (vs tests)?
- **F — invariants & hardening:** Walk invariants 1/4/9/10. Is the flat-JKS-password decision (Route 1) an acceptable interim, and is the deferral honestly bounded (not silently load-bearing)?
- **G — test gaps:** Do the tests actually *prove* the load-bearing claims (operator_id reaches the session end-to-end with a non-identity value; fail-closed on no-cert; `build_fn_sign` well-formedness)? What would a paranoid reviewer demand before this crypto seam merges?

## Output format

Numbered findings, each: **Severity** (Critical/High/Medium/Low/Info) · **Category** (A–G or other) · **file:line** (cite the real diff/current content; do NOT invent line numbers) · **the concern** (one paragraph) · **suggested fix** (1–2 lines). If a category has nothing actionable, say so. End with one sentence: **"Recommend MERGE"** / **"Recommend MERGE WITH FOLLOW-UPS"** / **"Recommend BLOCK"** + one-sentence justification.
