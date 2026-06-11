# Backup / Restore Runbook (RS-4, audit pass-2 item 4)

**Audience:** field operators / on-call.
**Scope:** the local PRRO gateway node — both SQLite files (`prro.db` main +
the secure DB).

> ⚠️ **Restore on a fiscal ledger is dangerous.** A restored snapshot is, by
> definition, *behind* what the node had at crash time. If the node resumes
> trading from a stale tip it will **reuse `lnd` values, break the MAC chain and
> double fiscal numbers against the DPS**. The node therefore refuses to trade
> after a restore until the **boot tip-guard** (PR-B) has confirmed the local
> ACK tail still matches the DPS. Do not bypass it.

---

## 1. What the node backs up (automatically)

When the supervisor runs with `[backup] enabled = true` (the default), a
background loop writes a verified, owner-only (`0600`) snapshot of **both** DB
files to `[backup] dir` (default `var/backups`) every `[backup] interval_seconds`
(default 3600), and prunes old snapshots per the retention policy
(`docs/operations/RETENTION_POLICY.md`).

Snapshot file names: `main-YYYYMMDD-HHMMSS-<8hex>.db` and
`secure-YYYYMMDD-HHMMSS-<8hex>.db` (UTC timestamp).

Each snapshot is taken with `VACUUM INTO` (a consistent point-in-time copy that
does **not** stop the cashier) and is verified with `PRAGMA integrity_check`
before it is kept; a snapshot that fails verification is deleted, never offered
for restore.

---

## 2. Restore procedure

1. **Stop the service.** Ensure the gateway process is fully stopped (no process
   holding the DB files). Use the orchestrator stop (systemd / docker) and
   confirm the process is gone.

2. **Pick the snapshot pair.** Choose the most recent `main-…` snapshot you
   trust, and the `secure-…` snapshot **closest in time to it** (operators are
   restored from the secure DB — without it you must re-onboard them).

3. **Verify the snapshots before copying** (optional but recommended):
   `sqlite3 <snapshot>.db 'PRAGMA integrity_check;'` must print `ok`.

4. **Copy the snapshots over the live files.** Replace the live `prro.db` with
   the chosen `main-…` snapshot and the live secure DB with the chosen
   `secure-…` snapshot (paths come from `[database] db_path` /
   `secure_db_path`). Remove any stale `-wal` / `-shm` sidecars of the live
   files before copying. Keep ownership/permissions owner-only.

5. **Start the service.**

6. **The boot tip-guard runs (PR-B).** On boot, per FN, after the normal
   recovery arms, the node takes the `server_fiscal_no` of its **newest
   submitted** receipt (its last doc in `SENT`/`KVT1`/`KVT2`/`ACK`) and asks the
   DPS (`last_chk`) whether that is still the tail. (The guard is skipped for an
   FN whose boot already exchanged with the DPS this pass — a fresh re-send or a
   recovery probe — since that contact already verified the tip.)
   - **OK** → `TIP_GUARD_OK` (INFO) → the node trades normally.
   - **STALE** (`TIP_GUARD_STALE_LEDGER`, CRITICAL) → `node_state.mode` is set to
     `BLOCKED` and the node **refuses fiscal commands** — see §3. This is the
     expected outcome if you restored a snapshot that is *behind* the DPS.
   - **DPS unreachable** → WARN, the node continues (offline-first); the guard
     re-runs on the next boot once the network is back.

7. **Confirm success.**
   - `node_state.mode` for each FN is **not** `BLOCKED` (e.g. `ONLINE`).
   - The audit log carries a `TIP_GUARD_OK` row for each FN (and **no**
     `TIP_GUARD_STALE_LEDGER`).
   - A test receipt fiscalises to `ACK`.

---

## 3. If the node comes up `BLOCKED` (`TIP_GUARD_STALE_LEDGER`)

This means the restored ledger is **behind the DPS** (or someone else fiscalised
on your FN). The node has correctly refused to trade to avoid `lnd`-reuse /
double-fiscalisation. **Do not** force it online or hand-edit the ledger.

- Capture the audit row (it records both the local last-ACK `server_fiscal_no`
  and the DPS-reported one).
- Escalate to support with that audit row. Resolution requires reconciling the
  gap with the DPS-side record (out of scope for field restore); only after that
  reconciliation may the FN be returned to service per the support procedure.

The node staying `BLOCKED` is the **safe** outcome — a stale node that traded
would corrupt the fiscal sequence irreversibly.

---

## 4. Notes

- The backup media inherits the security posture of the live disk (the secure DB
  holds a plaintext key password until KMS lands — see
  `docs/operations/RETENTION_POLICY.md`). Treat snapshots as sensitive.
- `[backup] enabled = false` disables automatic snapshots — then restore depends
  on whatever external backup the operator arranged.
- **Known limitation (residual).** The tip-guard verifies the *submitted* tail
  (the newest `SENT`/`KVT1`/`KVT2`/`ACK` doc). It does **not** cover a snapshot
  that captured a *pre-wire* doc (`PREPARED`/`SIGNED`) which is then restored: on
  boot the recovery arms re-drive that doc to the DPS *before* the guard, so an
  `lnd` re-use is possible before any tip check. Placing the guard before the
  recovery arms was rejected (it would false-block normal crash recovery).
  Mitigation: keep `[backup] interval_seconds` short relative to the (brief)
  pre-wire window. Full closure (a pre-arms, ACK-tail-only check) is tracked
  separately, after the legacy review.
- `[backup] tip_guard_enabled = false` is a kill-switch for the boot tip-guard
  (for false positives in the field): the node then trades after restore without
  the DPS tip check — use only under support guidance.
