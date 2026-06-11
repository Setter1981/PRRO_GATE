# Backup Retention Policy (RS-4, audit pass-2 item 4)

## 1. The ledger is never deleted

Fiscal data — the `fiscal_documents` ledger, the MAC chain, offline codes and
their consumption, the audit log — is **never** pruned, rotated or aged out by
the gateway. It is legally-retained fiscal evidence and only ever grows. Nothing
in this policy, and no operation in the product, deletes ledger rows.

Retention here concerns **snapshots only** — the copies the backup loop writes
to `[backup] dir`.

## 2. Snapshot retention

The backup loop snapshots both DB files (`main` + `secure`) every
`[backup] interval_seconds` and then prunes old snapshots with two knobs:

| Knob | Default | Meaning |
|------|---------|---------|
| `[backup] keep_last` | 30 | The disk-usage **cap**: at most this many most-recent snapshots survive **per label**. |
| `[backup] max_age_days` | 14 | Additional **expiry**: snapshots older than this are deleted even when within the cap. |

A snapshot is **kept only while it is BOTH within the `keep_last` most-recent
AND younger than `max_age_days`** (architect review ruling 2026-06-11: the
earlier AND-delete reading would retain `interval × max_age` full DB copies —
an hourly node would hold ~336 snapshots per label and exhaust edge-hardware
disks). Deleting by age is safe: pruning runs only **after** a successful
snapshot, so a fresh copy always survives the pass — a node that was off for
months never prunes itself to zero.

**Prune only touches our files.** Pruning matches strictly the
`<label>-YYYYMMDD-HHMMSS-<8hex>.db` name pattern; any other file in the backup
directory (operator notes, a different label, an unrelated `.db`) is never
touched.

## 3. Security posture of the backup media

**Snapshots inherit the security posture of the live disk.** Each snapshot is
written owner-only (`0600`, the same posture the live secure DB carries under
HIGH-AUDIT-01), but:

- The **secure DB is included** in the backup (turnkey restore matters: without
  it, a disk death forces a full operator re-onboarding). The secure DB today
  holds a **plaintext** key password (the JKS/key-store password) — this is the
  known **finding #6**; envelope-encryption via a KMS is planned post-pilot.
  Until then, a secure-DB snapshot is exactly as sensitive as the live secure
  file: anyone who can read it can read that password.
- Treat the backup directory and any off-node copies as **sensitive**: restrict
  access, and prefer full-disk encryption on the backup medium.

## 4. Recommendation: a second physical device

The backup loop emits a WARN audit (`BACKUP_ON_SAME_DEVICE`) when `[backup] dir`
sits on the **same physical device** as the live DB — because a single disk
failure would then destroy the ledger **and** every snapshot at once.

**Strongly recommended:** point `[backup] dir` at a **second physical device**
(a separate disk, a mounted USB, or a network path the operator manages). The
gateway does not care whether the path is local or networked — it just writes
files there. Off-node/off-site copies are the operator's responsibility and are
out of scope for the gateway itself.
