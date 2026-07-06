# LIVE DPS Smoke Runbook

W4-Z4 Pilot Readiness artifact 4 (`docs/architecture/W4_Z4_PILOT_READINESS_STABILIZATION.md` §4).
This runbook is the **operator-run procedure** for executing the live DPS smoke that
proves the native Rust fiscal cycle against the real DPS **test** cabinet.

> **Provenance / merge state (REFRESHED 2026-07-05).** `feat/m4-w4-z3-dps-extended-smoke`
> **MERGED to `main` via PR #112** — the harness `rust/prro/tests/live_dps_extended_smoke.rs`,
> the `live-dps` Cargo feature, and the proven `SHIFT_OPEN → SELL → Z_REPORT` cycle (§4.6)
> are all **present and runnable on `main` HEAD**. The connect/probe-only
> `live_smoke_w12_hardening.rs` also remains. The original note ("PENDING MERGE") is
> obsolete; commands/env/gates below apply to `main` as-is.

> **Pilot gate verdict: NO-GO (read first).** This is a **branch/technical wire smoke, NOT
> a pilot authorization** — the pilot gate verdict on `rust-gateway` HEAD is **NO-GO** (see
> §4.9 Hard-Blockers). A green run here proves the native crypto+wire profile only; it does
> **not** clear the gate.

> **Scope honesty (REFRESHED 2026-07-05).** This smoke proves the **WIRE only**. It seeds a
> `PREPARED` document directly and drives it through `reconcile_pending_with` →
> `dispatch_prepared_via_chain` → `stage_sign` → `stage_send`. It **bypasses
> `stage_acquire`** (no ingress, no lease, no LND allocation, no canonical-hash
> idempotency). **UPDATE (A′.1 pieces 2/3, PRs #224/#225): the online shift-lifecycle
> drivers are now WIRED at stage level** — `stage_acquire` mints the shift + edges 1/8,
> `stage_send`'s 4-b Sent-block confirms edges 3/10 (`Opening→Opened`, `Closing→Closed`).
> **Consequence for THIS legacy procedure:** because the seed-`PREPARED` path bypasses
> `stage_acquire`, a shift doc driven this way carries **no `shift_id` and no `Opening`
> shift row** — the piece-2 confirm hook will emit `SHIFT_CONFIRM_EDGE_DRIFT` (**CRITICAL**
> audit) on its SENT commit. The Sent commit **stands** (post-wire no-rollback policy) and
> the smoke still passes; the CRITICAL entry is an **expected artifact of the acquire
> bypass**, not a live defect. A refreshed procedure that enters through `stage_acquire`
> (exercising the real shift wiring end-to-end) is the recommended Tier-1 re-run once A.3
> PR-A (advance-at-SEND core) merges — that single run then validates BOTH the shift
> wiring and the new seed semantics against the live cabinet. This smoke still does **not**
> prove offline limits or channel-switch guards.

---

## §4.1 Purpose

Manual **live DPS acceptance** for the native Rust write-path, not mock validation.

The smoke drives the REAL Rust write-path against the REAL DPS test server
(`cabinet.tax.gov.ua:9443`) with a REAL signing key and the **native** `prro_crypto`
in-process signer (NO jkurwa sidecar — that architecture is dead). It exists to prove the
native fiscal cycle **`SHIFT_OPEN → extended SELL → Z_REPORT`** is ACCEPTED by live DPS,
not just by the mock channel and byte-goldens.

Each fiscal doc traverses the proven ФСКО state path:

```
PREPARED → SIGNED → SENT
```

driven by the production write-path stages `stage_sign` (native ATTACHED CMS over the
canonical CP1251 XML) then `stage_send` (`sendChkV2`). `KVT1 / KVT2 / ACK` confirmation
of the wire submit is the online-confirm path; this smoke asserts the DPS **ACK on the
synchronous `sendChkV2`** (state reaches `SENT` with a populated `server_fiscal_no`).

---

## §4.2 The TRIPLE GATE (this file never runs by accident)

> **Branch caveat.** The harness `live_dps_extended_smoke.rs` and the `live-dps` feature
> described here exist on `feat/m4-w4-z3-dps-extended-smoke` (PENDING MERGE); they are NOT
> on `rust-gateway` HEAD yet. The gates below describe the harness on that branch.

The harness `live_dps_extended_smoke.rs` cannot touch live DPS unless **all three** gates
are armed simultaneously:

1. **Cargo feature** — the whole file is `#![cfg(feature = "live-dps")]`, so it does not
   even **compile** without `--features live-dps`.
2. **`#[ignore]`** — every test carries `#[ignore]`; opt-in requires `-- --ignored`.
3. **`PRRO_LIVE_DPS=1` env kill-switch** — every test body calls `live_armed(...)`, which
   prints a `SKIP` line and returns OK unless this env var equals `1`. So even a stray
   `--ignored` run still cannot reach the wire.

### Test-host default-deny allowlist

On top of the triple gate, `live_armed()` parses the **real URI host** of the resolved
endpoint and applies a **default-deny allowlist**:

- ALLOWED: host exactly `cabinet.tax.gov.ua`, or a `.cabinet.tax.gov.ua` subdomain, or a
  `*-cabinet.tax.gov.ua` dev cabinet.
- REFUSED (hard `panic!`): every other host — including **every production endpoint**
  (`prro.tax.gov.ua`, the legacy `prro2.tax.gov.ua`, `fs.tax.gov.ua`) AND lookalikes such
  as `cabinet.tax.gov.ua.evil.com`.

Default-deny is deliberate: a prod-host **blocklist** would miss variants like `prro2`.
The host parser (`host_of`) isolates the **authority** before stripping userinfo/port, so
query/fragment tricks like `https://prro.tax.gov.ua:9443?x=@cabinet.tax.gov.ua` correctly
resolve to the prod host `prro.tax.gov.ua` and are **rejected**. This is regression-pinned
by the (non-live, always-on) unit test `host_of_isolates_authority_and_blocks_prod_tricks`.

---

## §4.3 Environment Contract

| Var | Required | Default | Meaning |
|---|---|---|---|
| `PRRO_LIVE_DPS` | yes (gate) | — | Must equal `1` or every test self-skips (kill-switch). |
| `PRRO_LIVE_DPS_HOST` | no | `https://cabinet.tax.gov.ua:9443` | DPS test endpoint (gRPC over TLS). Parsed host must pass the test-cabinet allowlist (§4.2). |
| `PRRO_LIVE_DPS_FN` | no | `4000162280` | Test fiscal number (`rro_fn`). |
| `PRRO_LIVE_DPS_JKS_PATH` | signing (pieces 2+) | — | Path to the JKS key container. Gitignored; mounted locally by the operator. |
| `PRRO_LIVE_DPS_JKS_PASS` | signing (pieces 2+) | — | JKS password. **NEVER logged.** |

Operational facts the harness pins:

- Signing key for FN `4000162280` is the JKS `key_13667753_…(2).jks` (TN `13667753`,
  signer «ГАЛЬЧУН МИКОЛА ДМИТРОВИЧ»).
- `LIVE_TN = "13667753"` — the **company ЄДРПОУ** is the receipt `TN`, NOT the signer's
  individual ІПН (`2790008754`). Mismatch → DPS rejects.
- Per-call deadline `SMOKE_TIMEOUT_SECS = 15` (covers slow TLS handshake + intermittent
  network; typical DPS response is ~1–3s).
- Connectivity (piece 1) needs only the gate; **all signed RPCs (pieces 2+) need the JKS
  env**. `load_signing_key` prints a SKIP and returns when the JKS env is absent, so a
  connectivity-only run still works with no key mounted.

Per the W4-Z4 STABILIZATION §4.2 pre-flight, before a live run also confirm: JKS file
exists with sane permissions, cert validity window is current, FN matches the
key/cert/operator binding, target host is the test cabinet (this one IS harness-enforced —
the §4.2 triple gate + allowlist), no unexpected open shift on the FN, local clock/NTP
sane, no active DPS cooldown (§4.7), and a DB/snapshot backup is taken if needed.

> **`PRRO_FISCAL_MODE=TEST` is a MANUAL operator preflight, NOT harness-enforced (DF-5).**
> The W4-Z3 harness does **not** define or check a `PRRO_FISCAL_MODE` env var. It gates only
> on `PRRO_LIVE_DPS=1` plus the test-cabinet host allowlist (§4.2); the local temp DB is
> seeded test-mode **internally**, independent of any such var. So setting/verifying
> `PRRO_FISCAL_MODE=TEST` here is purely an operator-discipline check — the harness will run
> regardless of its value. **Required pilot fix (deferred to the `feat/m4-w4-z3` branch):**
> add a hard harness check that fails fast unless `PRRO_FISCAL_MODE=TEST`. This is recorded
> as a pilot Hard-Blocker (see §4.9).

---

## §4.4 Secrets Hygiene

Rules enforced by the harness and required of the operator:

- **JKS password via env only** (`PRRO_LIVE_DPS_JKS_PASS`); it is **never logged** by any
  test body or helper. The harness never prints the password, `param_d`, or any private
  container bytes.
- **Key files are gitignored** — the operator mounts them locally and points
  `PRRO_LIVE_DPS_JKS_PATH` at one. Never commit key material.
- **Load the pass with `read -rsp` (no echo) into an exported var, never inline on the
  command line** (DF-7) — inline secrets leak to shell history and `ps`/`/proc`. Arm a
  `trap 'unset PRRO_LIVE_DPS_JKS_PASS' EXIT` and still **unset the env vars by hand after
  the run** (see §4.5 for the exact pattern).
- Allowed in logs: hashes, SKI, cert fingerprint, public cert metadata, truncated IDs, the
  MAC seed hex, `server_fiscal_no`. The harness prints the MAC seed (a SHA-256 of the prior
  check) and the DPS-assigned `server_fiscal_no` — both are public-chain artifacts, not
  secrets.

---

## §4.5 Execution Steps — exact cargo invocations

All commands run from the Rust workspace root.

**Load the JKS password securely first (DF-7).** Never paste the password inline on the
command line (it lands in shell history and `ps`/`/proc` for the lifetime of the process).
Read it into an exported var with no echo, and arm a trap to scrub it on exit:

```bash
# Prompt with no echo; export so the cargo child process inherits it.
read -rsp 'JKS password: ' PRRO_LIVE_DPS_JKS_PASS && echo
export PRRO_LIVE_DPS_JKS_PASS
# Scrub on shell exit (and unset by hand after the run — see §4.4 / §4.8b).
trap 'unset PRRO_LIVE_DPS_JKS_PASS' EXIT
```

The shared run pattern for every live test then references the var (never the literal):

```bash
PRRO_LIVE_DPS=1 \
PRRO_LIVE_DPS_JKS_PATH="/abs/path/key_13667753_13667753 (2).jks" \
  cargo test -p prro --features live-dps \
    --test live_dps_extended_smoke <TEST_NAME> -- --ignored --nocapture
```

(`PRRO_LIVE_DPS_JKS_PASS` is already exported, so it is inherited — do not re-state it
inline.)

> `--nocapture` is mandatory — the diagnostics (DPS code, `server_fiscal_no`, MAC seed,
> `transport_trace`, last 8 audit rows) print to stdout and are the operator's evidence.

> If `cargo` is not on PATH, prefix with `export PATH=/home/setter/.cargo/bin:$PATH` or use
> `rtk proxy cargo`.

### Compile-only (CI-safe; NEVER executes the wire)

```bash
cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run
```

This is the §3.9 / §4.x **Live DPS Compile Gate**: it must compile in CI with `--no-run`
and must **not** execute.

> **NOT runnable on `rust-gateway` HEAD.** The `live-dps` feature and
> `live_dps_extended_smoke` test target exist only on `feat/m4-w4-z3-dps-extended-smoke`
> (PENDING MERGE). On HEAD this command fails (no such feature/target); it becomes the
> binding static gate **only after that branch merges**. The connect/probe harness that
> DOES exist on HEAD is `live_smoke_w12_hardening.rs` (`--features test-support`).

### The smoke sequence (run sparsely, in order, by hand)

| # | Test name | Wire? | What it proves |
|---|---|---|---|
| 1 | `live_smoke_1_connect_probe` | read | `GrpcDpsChannel::connect` (TLS handshake + HTTP/2 SETTINGS) + a `last_chk` with a dummy `CheckSignBlob`. DPS rejects the dummy sign with a typed app-level error — which itself proves TLS + HTTP/2 + gRPC + response-parse. Only `DpsError::Transport` is a FAIL. No real signing, no fiscal mutation. |
| 2 | `live_smoke_2_last_chk_real` | read | First native-signed RPC: build a REAL `rro_fn_sign` (FN string signed with the operator EDS, ATTACHED CAdES-BES + signingTime — the exact profile `sendChkV2` requires) and read the FN chain tip. PASS = `Ok(CheckAck)`. A `-1 ERROR_VEREFY` fails loudly (attached-CMS profile mismatch). |
| 3 | `live_smoke_3_mac_seed` | read | MAC-chain seed bootstrap: `lastChk.data_sign` (prev check CMS) → `extract_econtent` → SHA-256 = the MAC the NEXT check must carry → `node_state::seed_prevhash` into a throwaway temp DB, read back. Closes the fresh-DB-vs-DPS-history gap (`-12 BAD_HASH_PREV`). Genesis FN (empty `data_sign`) = nothing to seed. |
| 4 | `live_smoke_4_extended_sell_native_sign` | **offline (stub DPS)** | Drives a PREPARED extended SELL through the production write-path with a `StubAckDps`. Asserts `PREPARED → SENT`; canonical tax groups `TX="1"`/`TX="2"` (driver 5/7 **translated**, raw numbers must NOT reach the wire — the W4-Z2a silent-fiscal-divergence proof); excise `<CA>` children + UKTZED `CZD`; `<TX>` summaries (`TXPR="20.00"`/`"7.00"`, `TXSM` aggregates); and the native ATTACHED CMS eContent is **byte-identical** to `PAYLOAD_XML`. Needs only the JKS env (no `PRRO_LIVE_DPS=1`, no wire). |
| 5a | `live_smoke_5a_status_probe` | read | `statusRro` + `lastChk` against the live cabinet — reports whether a shift is already OPEN on the FN and the chain tip, so the operator can decide if `SHIFT_OPEN` (5b) is the right next move. Read-only. |
| 5b | `live_smoke_5b_shift_open` | **LIVE (mutates chain)** | Seeded PREPARED `SHIFT_OPEN` (rides as `ServiceChk(3)`, `local_number=0`) through `reconcile_pending_with` against the live channel: `lastChk` → seed MAC from chain tip → reconcile → `sendChkV2`. ACK advances to `SENT` + `server_fiscal_no`. **Opens the DPS-side shift.** |
| 6 | `live_smoke_6_extended_sell` | **LIVE (mutates chain)** | Requires an OPEN shift (run 5b first; asserts `statusRro.open_shift`). Drives the piece-4 extended SELL (driver 5→1, 7→2 + excise + UKTZED) against live DPS (`Chk`, `local_number=1`). ACK → `SENT` + `server_fiscal_no`. |
| 7 | `live_smoke_7_z_report` | **LIVE (mutates chain)** | Requires an OPEN shift with the SELL fiscalized (run 5b → 6 first). `Z_REPORT` (`ZReport`, `local_number=2`) → `sendChkV2`. ACK → `SENT`, **closes the DPS-side shift**. |

**Run order for a full cycle:** `5b → 6 → 7` (each its own throwaway temp DB; DPS chain
state lives server-side; each doc re-seeds its `<MAC>` from the live `lastChk` chain tip —
the WebCheck model: trust DPS, not local state — so the chain advances automatically;
`local_number` is sequential `0 → 1 → 2`).

Example — the live SHIFT_OPEN (piece 5b). Assumes `PRRO_LIVE_DPS_JKS_PASS` was already
loaded via the `read -rs` + `export` + `trap` pattern above (DF-7); never inline the pass:

```bash
PRRO_LIVE_DPS=1 \
PRRO_LIVE_DPS_JKS_PATH="/abs/path/key_13667753_13667753 (2).jks" \
  cargo test -p prro --features live-dps \
    --test live_dps_extended_smoke live_smoke_5b_shift_open -- --ignored --nocapture
```

---

## §4.6 The PROVEN full-cycle result (branch-proven 2026-05-29)

The complete native fiscal cycle was accepted by live DPS. **These results were proven on
branch `feat/m4-w4-z3-dps-extended-smoke` (PENDING MERGE to `rust-gateway`) — they are
branch-proven, NOT HEAD-wired.** The harness that produced them is not on `rust-gateway`
HEAD yet (see Provenance note at the top):

| Step | Test | DPS-assigned `server_fiscal_no` (branch-proven 2026-05-29) |
|---|---|---|
| SHIFT_OPEN | `live_smoke_5b_shift_open` | `1g41M3jDt-Q` |
| extended SELL | `live_smoke_6_extended_sell` | `AOBSkplfIUU` |
| Z_REPORT | `live_smoke_7_z_report` | `L2AMnY2MkmA` |

Each doc reached state `SENT` with a populated `server_fiscal_no`. The MAC was seeded per
send from the live `lastChk` chain tip (per the doc-comment WebCheck model). This is the
proof that the native `prro_crypto` ATTACHED CMS over CP1251 canonical XML, the extended
ФСКО SELL surface (driver→canonical tax translation, excise `<CA>`, UKTZED `CZD`,
`<TX>` summaries), and the `sendChkV2` wire mapping are all accepted by the real server.

---

## §4.7 Rate-limit guard

The DPS test server returns **`status=-4`** after too many errors, with a **5+ minute
per-FN cooldown** (operator memory `project_dps_rate_limit`). Every live test treats
`DpsError::Server { code: -4, .. }` as a **SKIP** (prints the message, returns OK) — not a
FAIL — so a rate-limited run does not pollute the test result.

Operator rules:

- **Run sparsely** and **by hand**.
- **NEVER in CI.**
- **NEVER in a loop** / never auto-retry against live DPS.
- If you see `-4`, **cool down 5+ minutes** before re-running.

---

## §4.8 Troubleshooting

### DPS application reject codes (surfaced in `transport_trace.server_status_code`)

The harness prints, on a non-ACK send, the latest `transport_trace`
`[outcome_kind, server_status_code, error_kind, error_message, server_fiscal_no]` plus the
last 8 `audit_log` rows. Key codes:

| Code | Meaning | Likely cause / fix |
|---|---|---|
| `-1` | `ERROR_VEREFY` | Signature/profile rejected. The ATTACHED-CMS profile (eContent / signingTime / **signing cert** / signed content) does not match what DPS expects. Verify `signing_cert()` (KeyUsage=digitalSignature) is embedded, NOT the encryption cert. |
| `-4` | rate-limit | See §4.7 — cool down 5+ min. Treated as SKIP. |
| `-12` | `BAD_HASH_PREV` | The `<MAC>` does not chain off the FN's previous check. Re-seed from the live `lastChk` chain tip (piece 3 / `seed_mac_from_lastchk`). The classic fresh-DB-vs-DPS-history gap. |
| `-14` | `NOT_REGISTERED_SIGNER` | **Was the cert bug.** The keystore holds both a signing and a key-agreement (encryption) cert; embedding the encryption cert makes DPS reject. Fixed by `signing_cert()` selecting the `digitalSignature` cert (prro_crypto PR #107). |
| `-15` | `NOT_OPEN_SHIFT` | A SELL/Z was sent without an open DPS shift. Run `live_smoke_5b_shift_open` first; confirm with `statusRro.open_shift` (5a). |
| `-6` / `-10` | Z-sequence | The Z report NUMBER must match the FN's per-RRO Z sequence. The smoke allocates `1` from a fresh temp DB; if DPS enforces a sequence this may reject — diagnostics carry the code. |

### Transient Z-reject observation

A documented observation: the **first** `Z_REPORT` was **REJECTED**, and a **retry was
ACCEPTED** without changing the payload. If a Z reject looks transient (not a clear
`-14`/`-15`/MAC error), a single re-run after cooldown is reasonable — but obey §4.7 (never
loop).

### PASS / FAIL classification (per STABILIZATION §4.7)

- **transport fail** (`DpsError::Transport`) — wire brokenness (TLS / DNS / network /
  server reset / deadline). The only hard FAIL on the read probes.
- **DPS application reject** — a typed `DpsError::Server { code, .. }`; classify by the
  table above.
- **CMS verify reject** — `-1 ERROR_VEREFY`; wrong crypto/wire profile.
- **rate-limit** — `-4`; SKIP + cooldown.
- **auth / key failure** — `extract_private_key` fails (wrong pass / not a JKS), or no
  signing cert in the keystore.
- **state / recovery failure** — doc did not reach `SENT`, or no `server_fiscal_no` on ACK.
- **clock / cert validity failure** — stale `business_ts` (`-8`-class date reject) or a cert
  outside its validity window. The smoke uses `iso_now()` for a fresh wire time.

### A2.4 stale-`PROCESSING` (202-hang) — reaper NOT yet built (interim: manual re-drive)

**Context (RS-3 A2.4 binding flip).** The write-path binding is now the live
`InlineWritePath` (`supervisor.rs` DI root — `UnimplementedWritePath` is retired).
Every real fiscal failure self-terminalises the inbox (REJECTED + audit) — the
four-variant gate (`tests/a3_final_binding_flip.rs`) proves this. Fiscal safety is
intact: a replay of a `PROCESSING` row NEVER re-fiscalizes (it resolves from the
ledger → `202 IN_PROGRESS` or the accepted receipt), and an unknown-truth arm
NEVER mis-terminalises a durably-`Sent` receipt as failed.

**The gap (AUD-L2 Phase-1 arm-table, ruling 2).** A narrow set of STRUCTURAL-breach
arms — an A4-unexpected inline `Noop`, a no-wire send race, or an `advance_to_ack`
fault, EACH combined with an empty ledger — deliberately leave the inbox
`PROCESSING` and return `500` (`REPLAY_LEDGER_DRIFT`). The design hands these to
"B1/boot recovery owns convergence", but the **RS-3 stale-`PROCESSING` reaper is not
yet implemented**. Until it lands, such a row hangs the client at `202 IN_PROGRESS`
with no auto-convergence. These arms are unreachable on a healthy boot — they need
a structural anomaly (a doc that vanished from the ledger, or a `Noop` that the
unique-idempotency-key inbox insert already precludes).

**Operator surface — watch for these CRITICAL audits (each names a stuck row):**
- `INLINE_NOOP_UNEXPECTED` — an A4-unexpected inline `Noop`;
- a structural-breach `REPLAY_LEDGER_DRIFT` 500 — inbox `PROCESSING` with no
  terminal `fiscal_documents` doc.

**Interim recovery (manual re-drive).** For a row stuck `PROCESSING` with no
terminal doc:
1. Confirm no accepted/terminal doc exists for the `request_id`
   (`SELECT state FROM fiscal_documents WHERE request_id = ?`).
2. ONLY if genuinely absent (a structural breach): under close supervision, an
   operator may reset that inbox row `PROCESSING → NEW` so the next request
   re-drives it, OR mark it terminal (REJECTED) so the receipt is re-submitted
   with a NEW idempotency key. NEVER reset a row whose doc is `Sent`+ (that would
   risk a double-fiscalize).

**Pilot policy (RULING 2).** Build the reaper BEFORE a WIDE pilot. A
close-supervision pilot MAY run with this manual surface as the interim.

### WL-1 — UNWIRED online shift-lifecycle drivers (key scope caveat)

This smoke proves the **WIRE only**. The seeded-`PREPARED` drive **bypasses
`stage_acquire`**. The gateway's **local online shift-state lifecycle is NOT wired**: on a
live online `SHIFT_OPEN` ACK, the local `shifts` row and `node_state.shift_state` are
**never flipped** (confirmed: W4-Z3 observed `node_state.shift_state` never opens online).
The shift edges `3 (Opening→Opened)`, `4`, `8 (Opened→Closing)`, `10 (Closing→Closed)`,
`11`, `12` are whitelisted but have **no production caller today**. Ambiguous online
`SHIFT_OPEN`/`Z` timeout → manual (edges 4/12) is unreachable. Local shift tracking after
ACK is intentionally out of scope here and is reported separately as the **M4
shift-state-wiring gap (WL-1)**.

Conversely, the foundations this smoke does NOT exercise but which ARE wired and
regression-pinned elsewhere include: the 162-cell `check_shift_guard` matrix; and the
Pattern C `OFFLINE_LOCAL_ACK` local-ack branch. The following foundations on this list need
the same split/qualifier the three arch caps carry — they are **UNWIRED IN PRODUCTION**, NOT
flat WIRED:

- **Drain-reject of an `OFFLINE_LOCAL_ACK` backlog on a pending-drain shift →
  `REQUIRES_MANUAL_RECONCILIATION` + `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL`**, and the
  **edge-5 drain-finalize-opens-shift** path. Both are **code-present + test-pinned**
  (`backlog_drain.rs:2191`) but **UNREACHABLE in production (DF-1)**: the escalation fires
  only `if shift_in_pending_drain` (`backlog_drain.rs:952`), keyed on a pending-drain
  `shift_state` that **production never sets** (the only prod `upsert_initial` seeds
  `ShiftState::CLOSED` — `boot_phase.rs:1304`; orphan-boot resolution only drives toward
  `CLOSED` — `boot_phase.rs:1491`; the only `OPENED` write in `src` is a `#[cfg(test)]`
  fixture, `admin.rs:903`). The only **narrowly** wired part is the doc-finalize-to-`Ack`
  arm — and even that is **moot in prod** (see below): under `CLOSED`, `(Sell, Closed) →
  ShiftNotOpen` REFUSE (`stage_acquire.rs:897`), and the offline path is unreachable
  end-to-end — `node_state` has **no Offline/GoingOffline mode setter** (only
  `set_mode_blocked_tx` / `set_mode_stop_mode_tx`), `OfflineSessionService::open_session`
  has **zero production callers**, and `stage_offline_ack` requires `Opened` + an active
  offline session (`stage_offline_ack.rs:268-318`). So mode never flips `Offline`, no
  offline session opens, no `OFFLINE_LOCAL_ACK` doc forms, no backlog exists, and `drain()`
  early-returns "no active session" / "empty backlog" — the `Opened → None` finalize arm is
  itself unreachable in prod. This is tracked as **DF-1** and surfaced in the §4.9
  hard-blocker list (matching MATRIX §5 / PLAYBOOK §9, incl. the offline drain-safety
  silent-absence).

Two further foundations on this list also need that qualifier — they are NOT flat WIRED:

- **Code-pool exhaustion** raises a typed `CodePoolExhausted` (WIRED + tested); but the
  `→ STOP_MODE` caller-routing off that error is **UNWIRED** (`stage_offline_ack.rs:315`
  surfaces the typed error; no production caller routes it to `STOP_MODE`). The only wired
  `STOP_MODE` driver is the **distinct** drain Tier-2 trigger
  `trigger_tier_2_stop_mode` (`backlog_drain.rs:2074`), not the code-pool path.
- **The force/senior reconciliation seams** — the primitive is WIRED + regression-pinned,
  but there is **NO production driver / operator entry-point today** (test-only).

### Crypto direction (do not mislabel)

- **OUTBOUND to DPS** = CMS **detached-named/ATTACHED SIGNED** over CP1251 canonical XML
  (`crypto/provider.rs::sign_cms_detached`, ATTACHED encapsulation for `sendChkV2`).
  Signature algorithm = **DSTU 4145-2002 (PB-257)**; hash = **GOST 34.311 / DSTU 7564
  (Kupyna)**. DSTU 4145 is a **SIGNATURE** scheme — **NOT encryption**.
- **INBOUND (KVT2)** = `unwrap_envelope` **DECRYPT** of DPS `EnvelopedData`
  (`crypto/provider.rs::unwrap_envelope`). **Encryption is INBOUND only.**
- The W4-Z3 live cycle was **signed-only** (ATTACHED CMS SignedData; `sendChkV2` accepted).
  Proven ФСКО path `PREPARED → SIGNED → SENT`.

---

## §4.8b Emergency Off-Switch

If a live run goes wrong (wrong target, unexpected reject cascade, suspected duplicate):

1. **Stop the run** — `Ctrl-C` the `cargo test` process. There is no background worker in
   the smoke (it is a single-shot `reconcile_pending_with` against a temp DB).
2. **Disarm the gate** — `unset PRRO_LIVE_DPS` (and the JKS env vars) so nothing else can
   reach the wire.
3. **Do not loop / retry** against live DPS (§4.7). A second send before a 5+ min cooldown
   risks `-4` and noise.
4. **Inspect DPS state** before any rerun: `live_smoke_5a_status_probe` (read-only) reports
   `open_shift` and the chain tip. Decide from there whether the shift needs a `Z_REPORT`
   (7) to close, or is already consistent.
5. **Collect evidence** — the temp DB is discarded on test exit, but the `--nocapture`
   stdout carries the `transport_trace` row, the last 8 audit events, the MAC seed, and any
   `server_fiscal_no`. Capture that stdout.
6. **Never** target a production endpoint to "check" — the allowlist (§4.2) refuses it by
   design; do not attempt to bypass it.

---

## §4.9 Pilot gate verdict — NO-GO

**This runbook is a branch/technical wire smoke; it does NOT authorize the pilot. The pilot
gate verdict on `rust-gateway` HEAD is NO-GO.** A green live run here proves only that the
native `prro_crypto` crypto + DPS wire profile is accepted by the test cabinet. The
authoritative Hard-Blocker list and exit-gate verdict live in the W4-Z4 readiness MATRIX (§5
exit gate) and the PLAYBOOK exit criteria; the blockers visible from this runbook's scope
are at minimum:

> **Blocker-status refresh (2026-07-05):** two of the five original blockers are CLOSED;
> the verdict stays **NO-GO** on the remaining three (seed-fork fix A.3 is in flight;
> offline reachability and the binding flip are outstanding).

- ~~**Online shift-lifecycle drivers UNWIRED (WL-1)**~~ — **CLOSED 2026-07-05**: A′.1
  pieces 3+2 (PRs #224/#225) wired create + edges 1/8 in `stage_acquire` and confirm edges
  3/10 in `stage_send`'s 4-b Sent-block. `shifts` / `node_state.shift_state` now flip on a
  live SENT-confirm. (Ambiguous-timeout → manual remains a piece-5/A2.5 residual.)
- ~~**Native ATTACHED crypto unmerged**~~ — **CLOSED**: `feat/m4-w4-z3` merged to `main`
  via PR #112; the live-accepted ATTACHED CMS signer + `live-dps` harness are on HEAD
  (see the refreshed Provenance note).
- **PRRO_FISCAL_MODE not harness-enforced** — manual operator preflight only; a hard harness
  check remains a required pilot fix (DF-5, §4.3). **OPEN.**
- **Offline drain-safety + manual-recon escalation UNREACHABLE IN PROD (DF-1)** — the
  drain-reject-of-`OFFLINE_LOCAL_ACK`-backlog → `REQUIRES_MANUAL_RECONCILIATION` escalation
  and the edge-5 drain-finalize path are code-present + test-pinned but **unreachable
  end-to-end**: no `node_state` Offline/GoingOffline mode setter, zero production callers of
  `OfflineSessionService::open_session`, `stage_offline_ack` requires `Opened` + an active
  session — and W10a/W10b (offline `SHIFT_OPEN` accept + reserve gate) are still absent.
  No mode flip → no session → no `OFFLINE_LOCAL_ACK` doc → no backlog → `drain()`
  early-returns. **OPEN — scheduled as A′.1 piece 4 + roadmap A′.3.** (Anchor refresh: prod
  bootstrap seeds `CLOSED` at `boot_phase.rs:1835` — the online half of the old "cannot
  transact at all" statement is superseded by the WL-1 closure above; SELL is admitted once
  a live SHIFT_OPEN confirms.)
- **INV-05/06 channel guards UNWIRED** and **INV-09/10 offline limits UNWIRED** — risk-accept
  only with an ops freeze / offline descoped + controlled (Appendix; canonical framing in the
  MATRIX/PLAYBOOK). **OPEN** (offline limits: W10a reserve gate lands with piece 4).
- **NEW (2026-07-05): online seed-fork fix (A.3, spec v3 LOCKED) not yet landed** — until
  A.3 PR-A merges, the online seed advances only at ACK; the sequential one-doc-at-a-time
  discipline of THIS runbook is fork-safe, but any concurrent/pipelined driving of two
  online docs is forbidden (AUD-L2-1a). After PR-A this line closes.

See the MATRIX §5 / PLAYBOOK exit criteria for the full Hard-Blocker list and the path to GO.

---

## Static / build gates (Rust-only stack)

The Python `src/prro_gateway` tree is a **dead reference** — there is no `pytest`/`ruff`
gate. The pilot gate is Rust-only:

```bash
# Static gate
cargo fmt --check
cargo clippy -p prro --features test-support --tests -- -D warnings
cargo clippy -p prro_crypto --all-targets -- -D warnings

# Build / feature matrix
cargo build -p prro --tests --features test-support
cargo test -p prro --features test-support

# Live-DPS COMPILE-ONLY (must pass in CI; must NOT execute)
# NOTE: live-dps feature + live_dps_extended_smoke target are on
#   feat/m4-w4-z3-dps-extended-smoke (PENDING MERGE) — NOT on rust-gateway HEAD.
#   This command only becomes a binding gate after that branch merges.
cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run
```

(Use `cargo test -p prro` scope for routine runs; full-workspace test only pre-merge.)

---

## Appendix — Legal invariants this smoke touches (`docs/LEGAL_INVARIANTS.md`)

The smoke exercises (WIRE-level) and/or is bounded by:

- **INV-02** — LND monotonic, no gaps (rollback = VOID, never reuse).
- **INV-03** — shift must be open before fiscal ops. The wired 162-cell `check_shift_guard`
  enforces `(Sell, Closed) → ShiftNotOpen`; the smoke instead **seeds an OPENED shift**
  locally and opens the DPS shift via 5b (the DPS `-15 NOT_OPEN_SHIFT` is the server-side
  expression).
- **INV-07** — idempotency mandatory. The single column is
  `ingress_inbox.idempotency_key`, `UNIQUE (fiscal_number, idempotency_key)`. The canonical
  hash is NOT the idempotency key. The server-side `local_number` / `server_fiscal_no` is a
  separate **DPS idempotency surface**.
- **INV-13** — offline doc provisional until DPS Ack (sign at DRAIN). Not exercised here
  (online path); noted for contrast with the offline branch. NB in the pilot there is **no
  drift**: the live Rust column is `offline_sessions.state` with
  `CHECK (state IN ('OPENING','OPEN','DRAINING','CLOSED','ABORTED'))`. The CHECK constraint
  is defined in migration `rust/prro/migrations/015_offline_normalize.sql:140`, and the
  value set is the `enums.rs:54` enum. (`offline_sessions.rs:225` is an `UPDATE` statement
  that *uses* the `state` column — repo-uses-state evidence only; it does **not** hold the
  CHECK.) So the value is **`DRAINING`**. The `status` column / `CLOSING` value is the
  **OLD/dead pre-015 shape** (migration 004 / Python `sql/001_hot_store_init.sql`) —
  migration 015 already normalized
  `status`/`CLOSING` → `state`/`DRAINING`. If you see `status`/`CLOSING` you are looking at
  the dead schema, not the pilot DB.
- **INV-16** — excise goods carry the excise mark + UKTZED code. Exercised at the WIRE
  level: pieces 4 and 6 (the extended SELL) build excise `<CA>` children and the UKTZED
  `CZD` field, and assert they reach the canonical XML / wire payload.

UNWIRED invariants relevant to scope (gap-marker / xfail, **no prod driver today**):
**INV-04** 9-state active-shift partial-UNIQUE index (Rust has only a non-unique
`ix_shifts_fn_state`); **INV-05** channel-switch-with-open-shift guard (frozen invariant #3,
not enforced in Rust); **INV-06** failover-only-outside-shift (explicit GAP,
`CHANNEL-FAILOVER-01`); **INV-09** ≤36h continuous-offline ingress freeze; **INV-10** ≤168h
monthly cap (column `current_month_offline_seconds` exists, no enforcement reader); and the
WebCheck 36h cert-expiry `SHIFT_OPEN` gate (spec §16.10). These are not reachable by this
WIRE-only smoke and are tracked separately.
