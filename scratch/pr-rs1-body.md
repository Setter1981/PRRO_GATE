## RS-1 — Runtime supervisor / composition-root (maintenance arm)

Wires the **maintenance arm** of the runtime spine behind `prro serve`: boot reconcile-once → spawn drain + return-online tick loops → supervise to graceful shutdown. This is the recovery/maintenance half only; it does **not** yet wire ingress→inbox→live write-path on *fresh* receipts (that is RS-2/RS-3).

### Safety seam
- Entirely gated by `config.supervisor.enabled` (**default `false`**). With the flag off, `prro serve` is byte-identical to the prior M1-idle behaviour — a clean rollback seam.
- MULTI-FN aware: boot reconcile and both tick loops fan out over **all** configured fiscal_numbers (tens of FNs in the target deployment).

### What it adds
- `runtime/supervisor.rs` — composition root: `run_with_registry` (reconcile-once + spawn loops) → `supervise_until_shutdown`.
- Boot reconcile-once with **fresh per-FN `fn_sign`** rebuilt per pass (signingTime must be current at the wire — never boot-cached).
- Drain + return-online tick loops with configurable interval and bounded graceful join.
- `SupervisorCfg` knobs (`enabled`, `drain_interval_seconds`, `probe_interval_seconds`, `shutdown_grace_seconds`) with clamped sane bounds.

### Review findings closed
Two external review passes + two external audits over the branch. All HIGH/BLOCK findings closed in-branch:

- **F7** (HIGH) — boot reconcile no longer fails-the-world on `OfflineRefusal{GoingOnline}` under runtime: it records the FN as deferred and lets the drain loop finish the GoingOnline→Online transition. Ctx-free `App::boot` stays fail-closed. Closes a self-perpetuating multi-FN spine brick.
- **F1** (HIGH) — supervisor now supervises loop-death: a tick loop dying before shutdown emits a `CRITICAL SUPERVISOR_LOOP_DIED` audit, joins the sibling under grace, and returns `Err` (fail-stop → systemd `Restart=on-failure`). Bounded graceful shutdown joins **both** loops under **one** shared grace deadline.
- **F2** — between-FN shutdown check inside the tick fan-out; configurable `shutdown_grace_seconds`.
- **audit-2 F1** (HIGH) — `prro_crypto::signing_cert()` made **strict**: selects the cert with `KeyUsage=digitalSignature`, **no `certs[0]` fallback** (that fallback re-opened the `-14 CryptBadSign` class). Two production callers now fail-closed on a non-signing cert.
- **audit-2 F2** — PII `Debug` redaction on `OperatorRow` / `NewOperator` / `AddOperatorInput` (INN, cashier name, encoded key password → `<redacted>`).
- **audit-2 F3** — audit durability: Critical registry audits (`OPERATOR_ORPHAN_FN`, `OPERATOR_KEY_LOAD_FAILED`) now propagate persist failures instead of swallowing them.

Deferred (non-blocking, low/info): probe-wire freshness test, CMS-skip micro-opt, password UTF-8 contract doc, boot-reconcile SIGTERM interruptibility, startup DPS thundering-herd.

### Verification
- Full workspace: **2188 passed / 0 failed / 14 ignored** (fresh rebuild, exit 0).
- `cargo clippy` / `cargo fmt --check` clean on RS-1 code.
- New tests: supervisor boot (multi-FN no-brick, defer-vs-fail-closed, loop-death audit), strict signing-cert selection (returns None on no-digitalSignature), session fail-closed on encryption cert, config clamp/default.

### Invariant check
- **#1** (no net/crypto in long SQLite tx): preserved — `fn_sign` built outside tx; W9b drain uses per-doc short transactions.
- **#2** (one FN = single-writer): preserved — reconcile_mutex per pass; loops fan out sequentially per FN.
- **#8** (recovery doesn't silently violate transitions): preserved — GoingOnline defer is explicit + audited, not silent.
- **#9** (graceful shutdown > fast): strengthened — bounded one-grace join, between-FN shutdown checks.
- **#10** (signing bypass only by config): strengthened — strict signing-cert removes accidental certs[0] drift.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
