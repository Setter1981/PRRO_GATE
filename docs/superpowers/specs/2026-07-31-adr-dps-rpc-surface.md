# ADR-DPS-RPC — the DPS RPC surface: which of WebCheck's eight we implement, and why

**Date:** 2026-07-31
**Status:** ACCEPTED
**Closes:** `bd PRRO_GATE-0ps` (P1) — "DPS proto drift vs WebCheck TaxGrpc decompilation", open since 2026-05-05 as a W3 sign-off gate.
**Splits out:** `bd PRRO_GATE-2pz` (P3) — `delLastChk` / `delLastChkId` support, by operator decision.
**Tooth:** `rust/prro/tests/dps_rpc_surface_pin.rs` — the decisions below are machine-checked; drifting the surface turns RED.

---

## 1. Context

WebCheck's decompiled `TaxGrpc` service (`ChkIncomeService.cs:15,257`) generates **eight** unary
RPCs. Our proto (`rust/prro/proto/fiscal_server.proto:8-14`) declares **five**. The ticket framed
that gap as "drift" and demanded a documented decision before pilot scope review.

Re-verified against the code on 2026-07-31 (the ticket body was written 2026-05-05 and predates
B6/B7, T=112 and CS-3):

| RPC | ours? | how it is used today |
|---|---|---|
| `sendChkV2` | ✅ | `DpsChannel::send_chk` **and** `send_chk_observed` (CS-3 §4.2: ONE physical call, two projections) **and** `ask_offline_codes` — T=112 rides the same RPC with `typCheck=3` / `ServiceChk`, **live-proven 2026-07-07** |
| `lastChk` | ✅ | `DpsChannel::last_chk`; also the substrate for `by_server_fiscal_no` (`PRRO_GATE-5js`) — DPS has no lookup-by-id RPC |
| `ping` | ✅ | connectivity probe |
| `statusRro` | ✅ | shift / online state |
| `infoRro` | ✅ | RRO descriptor |
| `sendChk` | ❌ | API **v1** submit — see D1 |
| `delLastChk` | ❌ | destructive last-receipt cancel — see D2 |
| `delLastChkId` | ❌ | destructive id-targeted cancel (`CheckRequestId`) — see D2 |

**On `verAPI`.** There is **no `verAPI` field on our wire.** The identifier appears nowhere in
`rust/prro/src/` except two prose comments (`xml/mod.rs:860,2059`). In WebCheck it is a *client
parameter that picks the RPC*: `Client.Check(verAPI, …)` routes v1 → `sendChk` and v2 → `sendChkV2`
(`TaxGrpc/Client.cs:57,114`), defaulting to `2` (`WebCheck/All.cs:20`, `ClassFiscal.cs:69`). So for
us the statement *"we are verAPI = 2"* is **exactly** the statement *"the only submit RPC we own is
`sendChkV2`"* — which is what the tooth asserts. Nothing needs to be added to the wire to "record"
it, and hard-coding a `verAPI` field would in fact be wrong: DPS never receives one.

---

## 2. Decisions

### D1 — `sendChk` (API v1): DEFER, with an explicit reachability criterion

**Decision: not implemented. We are v2-only.**

The ticket's rationale ("WebCheck defaults to v2, so new pilots can start on v2") is correct but
incomplete: WebCheck does **not** only use v1 when configured to. It **downgrades dynamically**, and
the exact condition matters (`WebCheck/SubmitPtrRobot.cs:73-84`, verbatim):

```csharp
int verAPI = All.A.verAPI;
string text2 = dd.ToString();                       // dd = the document date, yyyyMMdd…
int num  = …text2[0..3]…;                           // year
int num2 = …text2[4..5]…;                           // month
if (All.A.verAPI == 2 && num < 2022 && num2 < 10)
{
    All.A.verAPI = 1;                               // ← v1 for this submit only
}
…
All.A.verAPI = verAPI;                              // restored right after
```

Two things worth stating plainly, because both are easy to mis-summarise:

1. The trigger is the **document date**, not the config — a v2-configured WebCheck still submits
   some documents over v1.
2. The condition is literally **`year < 2022` AND `month < 10`**, *not* "earlier than 2022-10". A
   document dated 2021-11 (month 11) is **not** downgraded; one dated 2019-03 is. That reads like a
   defect in WebCheck's intent, but the *observed* behaviour is what it is, and we record the
   behaviour, not the guess at the intent.

**Reachability for us: nil.** Our documents are stamped with current Kyiv wall-clock time; a
`business_ts` with `year < 2022` cannot arise from any live path, and the offline backlog is bounded
by legal limits measured in hours, not years. So the only way v1 could matter is an operator
migrating an **existing** WebCheck contour whose config pins `apiver=1`.

**D1a — policy for an `apiver=1` config: REFUSE, do not auto-migrate.**
Silently promoting such a contour to v2 changes the protocol contract of a live cash register
without the operator knowing. The refusal must name the reason and point at this ADR. If a real
contour ever demands v1, that is a scoped transport slice (one RPC + its decode) gated on this ADR
being updated — not a config flag someone flips at 3 a.m.

### D2 — `delLastChk` / `delLastChkId` / `CheckRequestId`: DEFER to `PRRO_GATE-2pz` (P3)

**Decision: not implemented; split out by operator decision as its own P3.**

These are **destructive** operations, and they are generated-but-private even inside WebCheck: the
public COM/1C surface exposes only `Open / Check / Ping / CheckLast` (`TaxGrpc/IClient.cs:7`).

The blocking reason is not effort, it is a **question we cannot answer from the decompilation**: our
ledger invariants assume **append-only** — the `lnd` sequence, the MAC chain and the issued-chain
walk all read that way. A DPS-side deletion may leave a hole in the server's numbering, or move the
server's chain tip. Until that is known, wiring the RPC would be building on an unverified
assumption about the peer.

**Therefore the first step of `2pz` is a PROBE, not code:** establish against the test cabinet what
DPS actually does to a fiscalised check — and specifically whether a deletion leaves a hole in the
numbering or perturbs the MAC chain. Only then does the design question ("`DpsAdminChannel` trait +
audit gate + operator confirmation") become answerable.

Note the classification asymmetry the ticket already anticipated: whatever shape this eventually
takes, it must **not** fold into `DpsChannel`. A destructive op reachable from the ordinary write
path is exactly the accident this defers.

---

## 3. Consequences

- The pilot ships a **five-RPC** DPS surface. Production parity for the fiscal path is complete:
  every M3a/M3b flow (submit, recovery `lastChk`, probe, status, info, and T=112 replenish) rides
  those five.
- `PRRO_GATE-0ps` is closed by this ADR; `2pz` carries the `delLast*` question forward with a probe
  as its first step.
- The surface is now **machine-checked** (`dps_rpc_surface_pin.rs`): three tests assert the exact
  five, the deliberate absence of the three, and that the only submit RPC is the v2 one.

## 4. Acceptance mapping (against the ticket's own criteria)

| ticket acceptance | where |
|---|---|
| decision recorded in ADR | this document |
| `sendChk` v1 implemented OR deferred **with a migration plan for `apiver=1`** | D1 + D1a (refuse, never silent-migrate) |
| `delLast*` implemented behind an admin trait + audit gate OR deferred to a follow-up | D2 → `PRRO_GATE-2pz`, probe-first |
| W3 sign-off references the decision | this ADR is the referenceable artefact; the tooth makes the reference enforceable rather than prose-only |

## 5. What this ADR does NOT decide

- Whether DPS's **own** numbering is append-only. That is the `2pz` probe's question; nothing here
  assumes an answer.
- Anything about `-3` retry / resend policy — that is `PRRO_GATE-6bj`, and its ticket text needs
  rewriting before it can be implemented (CS-3 S7-1 R6 deliberately removed auto-redrive).
