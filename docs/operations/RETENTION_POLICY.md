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
| `[backup] keep_last` | 30 | Always keep at least this many most-recent snapshots **per label**, regardless of age. |
| `[backup] max_age_days` | 14 | May delete snapshots older than this — **but only those already beyond `keep_last`**. |

The two conditions are **ANDed**: a snapshot is deleted only if it is *both*
beyond the `keep_last` most-recent *and* older than `max_age_days`. So a node
that snapshots hourly keeps ≥30 snapshots at all times, and also keeps anything
from the last 14 days even if that exceeds 30.

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
