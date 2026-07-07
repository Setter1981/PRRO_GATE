# A′.3 Offline Drill — Operator Runbook (PR-O1)

**Scope:** the offline operator surface landed in A′.3 PR-O1 — node mode-seam
(`go-offline` / `go-online`), atomic offline-session open, and manual offline
code provisioning (`seed-codes`). The live operator door is **gated off** in O1
(`FULL_OFFLINE_SURFACE_READY = false`); it is enabled together with the drain
path in O2. This runbook documents the **manual code-seeding affordance** used
for the pilot offline drill.

---

## ⚠️ Seed ONLY real DPS-issued code ranges

`prro admin seed-codes` populates the FN's offline reserved-number pool. You MUST
seed **only the exact ranges the DPS actually issued for this fiscal number**
(from the DPS cabinet / prior provisioning records). **Never invent numbers.**

**Why this is load-bearing:** offline receipts consume codes from this pool and
are held as `OFFLINE_LOCAL_ACK`. When connectivity returns, the drain sends each
held receipt to DPS carrying its consumed offline number. If a receipt carries a
number DPS never issued, **DPS rejects it, and the drain escalates the shift to
`RequiresManualReconciliation`** — an invented range therefore produces a cascade
of RMR escalations across the whole offline backlog. There is no safe recovery
other than manual reconciliation with the tax authority.

**This command is a pilot-drill affordance, NOT a permanent mechanism.** The
production path is a DPS *ask-codes* request (the named follow-up, co-scoped with
the live campaign): the first live contact with the test DPS fixes the ask-codes
contract, after which a transport-PR replaces manual seeding.

---

## Commands

```bash
# Provision the offline code pool (ONLY real DPS-issued ranges).
prro admin seed-codes \
    --config /etc/prro/config.toml \
    --fn 1234567890 \
    --first 100 --last 199 \
    --reason "DPS-issued offline range 100..199 imported from cabinet 2026-07-07"

# (O2 only — gated off in O1) operator-initiate offline.
prro admin go-offline --config … --fn 1234567890 --reason "…"
# (O2 only — gated off in O1) operator-initiate return-online.
prro admin go-online  --config … --fn 1234567890 --reason "…"
```

`seed-codes` validates the range (positive, `first <= last`) and **loud-rejects
any overlap** with codes already in the pool (each range must be seeded exactly
once). Every invocation writes a `ADMIN_SEED_OFFLINE_CODES` Critical audit row
carrying the range. In O1, `go-offline` / `go-online` fail closed with
`OfflineSurfaceNotReady` until the drain path lands (O2, ship-together).

---

## Drill shape (reachability, O1)

`boot → online SHIFT_OPEN → GO_OFFLINE (mode + session) → offline SELL/RETURN →
OFFLINE_LOCAL_ACK`. Drain / return-online convergence is O2. In O1 the offline
receipts legitimately rest at `OFFLINE_LOCAL_ACK` with an OPEN offline session —
this is a legal durable state, not a stuck one.
