# External AUDIT request — RS-1 (runtime spine, whole deliverable)

You have **local read access** to `/mnt/d/PRRO_GATE`. This is a **full, independent
audit of the entire RS-1 work** before it merges into `rust-gateway` — not a delta
re-review. Read the code directly; challenge it.

## What RS-1 is (and is NOT)

RS-1 is the **runtime supervisor / composition root** for a Ukrainian PRRO fiscal
gateway: the missing "runtime spine" that wires the already-built maintenance
primitives into a live `prro serve`. It delivers the spine's **maintenance arm**:

1. a per-FN composition root — operator EDS key loading + per-FN `DpsChannel` +
   `SigningContext` (Pieces 1–3b);
2. a supervisor that runs boot crash-recovery ONCE, then drives the offline-backlog
   **drain** loop + the **return-online probe** loop, with graceful shutdown
   (Pieces 5a–5e + fix round F1/F2/F7).

**Scope boundary (do not flag as "missing"):** RS-1 operates over EXISTING DB rows
(reconcile / drain / probe). It does **not** wire ingress → inbox → live write-path
on FRESH receipts — that is RS-2/RS-3, deliberately out of scope. The supervisor is
**gated by `config.supervisor.enabled`, default FALSE** → the binary boots and idles
(byte-identical M1 behaviour) until an operator flips it on for the pilot.

**Deployment reality:** MULTI-FN gateway — serves tens of fiscal_numbers
concurrently (~50 sites / ~70 cash registers, each register = its own FN). Tick
loops + boot reconcile fan out sequentially over ALL registered FNs.

## Diff / commits / files

- Branch `feat/rs1-runtime-supervisor`, base (merge-base with rust-gateway) `a940520`.
- Whole RS-1:  `git -C /mnt/d/PRRO_GATE diff a940520..HEAD -- rust/`
- 14 commits `f977b8e..732fb8d` (Piece 1 config → 2/3/3b crypto seam → 5a–5e
  supervisor → fix round F7/F1/F2 + docs).
- Core source: `rust/prro/src/runtime/{supervisor.rs,key_loader.rs,bindings.rs}`,
  `rust/prro/src/crypto/{session.rs,errors.rs}`, `rust/prro/src/config/mod.rs`,
  `rust/prro/src/main.rs` (Cmd::Serve gate),
  `rust/prro/src/app.rs` (reconcile_pending_inner),
  `rust/prro/src/services/reconciliation/boot_phase.rs`.
- Tests: `rust/prro/tests/{rs1_supervisor_boot,rs1_build_fn_sign,bindings_registry_build,
  operator_key_load_failure_audits,crypto_provider_smoke,handler_503_on_missing_operator}.rs`.
- Ops: `docs/operations/admin-runbook.md` §6c.

## Frozen invariants (must not be violated)

1. No network or crypto calls inside a long SQLite write transaction.
2. One `fiscal_number` = one logical single-writer write-path.
4. Idempotency is mandatory.
5. Offline must respect time + code limits.
8. Recovery / reconciliation must not silently violate state transitions.
9. Graceful shutdown matters more than finishing fast.
10. Local signing may be bypassed only by explicit profile/config, not code drift.

## Audit axes — be a skeptic, cite file:line

### A. Crypto + secret discipline (Pieces 2/3/3b — `key_loader.rs`, `session.rs`, `errors.rs`)
1. **`operator_id` is the cashier INN = PII.** It must reach ONLY `audit_log`, never
   process logs / tracing / `Debug`. Audit every `Debug`/`Display`/error path:
   `SigningSession`, `SealedMaterial`, `CryptoError`/`JksUnseal`. Any leak?
2. **The `-14 CryptBadSign` trap.** `from_extracted` MUST select the SIGNING cert
   (`signing_cert()`, KeyUsage=digitalSignature), NEVER `certs[0]` (an encryption
   cert) — wrong-cert was a live DPS rejection root-cause. Verify the selection +
   `MissingSigningCert` fail-closed-on-absence. Is there any path that could embed
   the wrong cert in the CMS?
3. **Key material handling.** `Zeroizing` of the private scalar; no plaintext key/
   password copies; no `#[derive(Debug)]` on secret-bearing types; JKS password
   borrowed (not `to_string`-copied). Any retained plaintext?
4. **`build_fn_sign`** (`rro_fn_sign` = attached CAdES-BES CMS over the FN string,
   `signingTime = now()`): is it rebuilt FRESH per call (per tick / per reconcile
   pass), never cached for the process lifetime? (RS-Q3 freshness — `signingTime`
   must be current at the wire call.)

### B. Composition root (`bindings.rs` — `build_from_db`, `BindingsRegistry`)
5. Cross-DB FK enforcement (operators in the SECURE db, fiscal_number_config in the
   MAIN db — no cross-DB transaction). Password decode correctness.
6. Orphan operator (operators row with no config) → CRITICAL `OPERATOR_ORPHAN_FN`
   + skip; configured-FN-without-operator → Info `OPERATOR_NOT_REGISTERED` + skip;
   key-load failure → typed audit + skip. Is any failure mode fatal-to-the-whole-
   registry when it should be per-FN skip (cf. the F7 class of bug)?
7. Single shared `Arc<dyn DpsChannel>` across all FN entries; `registry.get == None`
   → caller must 503, never panic.

### C. Gate / rollback / config (Piece 1 — `config/mod.rs`, `main.rs`)
8. `enabled = false` ⇒ byte-identical M1-idle (verify `Cmd::Serve` branch + that
   `App::boot` does NOT reconcile). `enabled = true` + blank `dps.endpoint` ⇒
   fail-closed boot error (no panic). Connect failure ⇒ hard boot fail (enabled
   path only).
9. All clamps (DPS timeout, drain interval, probe interval, shutdown grace) avoid
   the 0-trap via hand-written `Default` (serde-default applies only on
   deserialize). Any field where a missing-table vs present-empty-table yields
   different effective values?

### D. Runtime lifecycle + shutdown (Pieces 5a–5e + F1/F2/F7 — `supervisor.rs`, `app.rs`)
10. Boot reconcile runs ONCE under the App reconcile mutex BEFORE any loop; it seeds
    node_state for configured FNs. F7: a `GoingOnline` FN is deferred to the drain
    loop (runtime path) but fail-closed under the ctx-free boot-gate. Verify no FN
    is silently lost (invariant #8).
11. F1: a tick loop dying before shutdown (panic) → CRITICAL `SUPERVISOR_LOOP_DIED`
    + `Err` → non-zero exit → orchestrator restart. F2: between-FN shutdown bail +
    single shared grace-timeout join (detach-not-abort on elapse). Any hang,
    deadlock, mis-join, or lost-task?
12. **Invariant #1** — confirm NO DPS/crypto call happens inside a long SQLite write
    tx anywhere in the reconcile/drain/probe paths the supervisor drives (fn_sign
    build + DPS calls must be OUTSIDE the tx; drain uses per-doc short txs).
13. **Invariant #2** — single-writer per FN: drain + reconcile serialize on the App
    reconcile mutex; the return-online PROBE is the intentional CAS-scoped exception
    (`Offline→GoingOnline` via a guarded `WHERE mode='OFFLINE'` CAS, deliberately
    NOT behind the mutex). Confirm this is race-safe, not a single-writer violation.
14. Node-mode state machine: are the `Offline / GoingOnline / Online` transitions
    driven by RS-1 (probe CAS up, drain CAS down) consistent + idempotent across
    restarts? Note the pre-existing W12 gap (a `GoingOnline` FN whose drain finalize
    is `DeferredKvt1` pre-W12 is not yet flipped to Online) — assess only whether
    RS-1 makes it WORSE.

## Already reviewed (history — but audit the WHOLE thing fresh)

- The crypto seam (2/3/3b) had 2 external reviews + an internal convergence round →
  all MERGE (PII Debug, password-copy, signingTime, SealKind, signing_cert()).
- The fix round (F7/F1/F2) had 2 external reviews + a 4-agent internal adversarial
  pass → all SHIP/MERGE; 1 internal-found defect (2× grace) fixed; 2 Low doc-drifts
  fixed. The F7 narrowing invariant is pinned by existing boot_phase tests
  (`branch_d_refuses_boot_on_going_online_mode` + `boot_in_offline_mode_reaches_per_doc_dispatch_not_branch_d_refusal`).

Treat that as context, not a reason to skip — independently re-derive the whole
composition's correctness.

## Deferred to follow-up (do NOT re-raise as merge blockers — calibrated low/info)

- F3 — boot `fn_sign` staleness across FNs (only FNs with a SENT backlog at boot
  send fn_sign on the wire; ~15–35s skew at ~70 FNs, within DPS tolerance,
  recoverable). Cheaper fix = JIT per-FN build, NOT a `RuntimeView` Cow refactor.
- F4 — wasted CMS sign on a skipped tick (~0.05% of a core at 70 FNs).
- F5 — supervisor-level probe-wire freshness test (constituents already covered;
  if added assert well-formedness + wire-reached, NOT cross-tick byte-inequality —
  DSTU signatures are randomized).
- Boot reconcile uninterruptible by SIGTERM (low); startup DPS thundering-herd (info).

## Verification done on our side

- `cargo test --workspace --features prro/test-support` on a FRESH rebuild =
  **2187 passed / 0 failed / 14 ignored** (158 suites).
- `cargo clippy` clean on the RS-1 prod code (residual warnings are pre-existing
  prro_crypto debt); `cargo fmt` clean on the RS-1 files.
- The full RS-1 pipeline was live-proven earlier (SHIFT_OPEN→SELL→Z accepted by the
  TEST DPS cabinet) for the crypto/sign path.

## Output

Findings as **Critical / High / Medium / Low / Info** with `file:line` anchors and,
for any real defect, the exact failing scenario. End with a merge recommendation
(SHIP / MERGE-with-follow-ups / BLOCK) and what, if anything, must change before
`supervisor.enabled = true` in the pilot vs before the merge of this gated-off code.
