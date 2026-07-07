# A′.3 Offline Drill — Operator Runbook (PR-O2)

**Scope:** the offline operator surface is **LIVE** as of A′.3 PR-O2
(`FULL_OFFLINE_SURFACE_READY = true`) — the node mode-seam (`go-offline` /
`go-online`), atomic offline-session open, manual code provisioning
(`seed-codes`), and the **return-online drain** that converges the offline
backlog. The door was gated off in O1 and flipped live in O2 together with the
drain path (ship-together: no live door without a reachable drain).

**Pilot prerequisite — the supervisor MUST run:** the drain / return-online /
convergence loops are spawned by `supervisor::run()`, which only runs when
`supervisor.enabled = true`. A pilot deployment MUST set this (it is also
required for ingress). With it set, `GO_OFFLINE → backlog → GO_ONLINE → drain →
ONLINE` converges automatically on the supervisor tick — no extra wiring.

```toml
[supervisor]
enabled = true
```

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

## 🚧 Pilot limitation — shift OPEN / CLOSE under offline is NOT yet available

The canonical Pattern C round-trip is **open the shift ONLINE**, then (net drop)
sell offline, then reconnect → drain → **close the shift ONLINE**. This is fully
supported.

**Opening or closing a shift *while the node is offline* is fail-closed** (the
`stage_acquire` pre-W10 guardrails refuse offline `SHIFT_OPEN` / `Z_REPORT` /
`SHIFT_CLOSE`). This lands in PR-O3 (offline shift-lifecycle edges 2/7/9). It is
an **availability limitation, not a fiscal hazard** — nothing incorrect is
signed; the operation is simply refused.

**Operator instructions until O3:**
- **The trading day starts with no network** → do NOT force an offline open.
  Wait for connectivity to open the shift online, or escalate per the outage
  procedure. (A shift cannot be opened offline.)
- **The network drops while a shift is open, near close time** → keep selling
  offline; **close the shift only after reconnecting** (GO_ONLINE → drain →
  online `Z_REPORT`). Do not attempt an offline close.

---

## Commands

```bash
# Provision the offline code pool (ONLY real DPS-issued ranges).
prro admin seed-codes \
    --config /etc/prro/config.toml \
    --fn 1234567890 \
    --first 100 --last 199 \
    --reason "DPS-issued offline range 100..199 imported from cabinet 2026-07-07"

# Operator-initiate offline (ONLINE → OFFLINE + opens the offline session).
prro admin go-offline --config … --fn 1234567890 --reason "net outage 10:30"
# Operator-initiate return-online (OFFLINE → GOING_ONLINE; supervisor drains).
prro admin go-online  --config … --fn 1234567890 --reason "connectivity restored 11:05"
```

`seed-codes` validates the range (positive, `first <= last`) and **loud-rejects
any overlap** with codes already in the pool. `go-offline` opens the offline
session atomically with the mode flip (no window where the node is offline but
has no session). All three write a Critical audit row.

---

## Drill shape (full round-trip, O2)

`boot → online SHIFT_OPEN → GO_OFFLINE → offline SELL/RETURN (→ OFFLINE_LOCAL_ACK)
→ GO_ONLINE → drain → ACK → online Z_REPORT (close)`. The return-online drain
converges each held receipt to `ACK`, closes the offline session, and returns the
node to `ONLINE`; the closing `Z_REPORT` aggregates **both** the online-issued and
the drained-offline receipts. The shift stays `Opened` throughout (it was opened
online); the offline shift-lifecycle states are O3.
