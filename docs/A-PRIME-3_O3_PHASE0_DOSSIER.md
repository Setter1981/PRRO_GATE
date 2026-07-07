# A′.3 PR-O3 — Phase-0 Dossier (offline shift-lifecycle, edges 2/7/9)

**Base:** `main @ bd3bfc1` (#237, phase A+A′ closed, door LIVE). **Branch:** `feat/aprime3-o3-offline-shift`.
**Status:** Phase-0 machine-verify done (self-verified per the Haiku-recon lesson). **STOP** on the α/β ruling before code.

---

## 1. Machine-verified findings (2a / 2b / 2c)

- **2a — `stage_offline_ack` Step-3 allowlist** (`stage_offline_ack.rs:276`): the fiscal-op allowlist is `Sell|Return|ServiceIn|ServiceOut|CashWithdrawal|XReport` → accept `{Opened, OpenedLocalPendingDrain}`; **`SHIFT_OPEN`/`SHIFT_CLOSE`/`Z_REPORT` fall to the `_` arm → `Opened` only**. ⇒ offline-ack'ing a shift-doc needs Step-3 widening (RED-first).
- **`check_shift_guard`** (`stage_acquire.rs:1014`): `(ShiftOpen, Closed, _) => None` — **offline SHIFT_OPEN already PASSES the guard**; the gap is downstream (shift-create is `if channel == Channel::Online`, `:706`). The α/β guardrails are `(ShiftClose, OpenedLocalPendingDrain, _) => OfflineShiftCloseNotSupported` (`:1038`) and `(ZReport, OpenedLocalPendingDrain, _) => ZReportBlockedBacklogDrainPending` (`:1044`), both with negative-pins.
- **2b — code consumption** (`stage_offline_ack.rs:332`): Step-6 `acquire_code_tx` is **NOT doc-type-gated** — a shift-doc through offline-ack would consume an offline code + stamp `offline_fiscal_no`. WebCheck nuance: DocType=8 (shift-open) does NOT increment `localchecknumber`. **→ sub-decision (below).**
- **2c — edge 2 form**: mirror the online create block (`stage_acquire.rs:706-758`, `create_shift_tx` + `apply_shift_transition(Created→Opening)`) with an OFFLINE branch: `channel==Offline && doc_type==ShiftOpen` → `create_shift_tx` + `apply_shift_transition(Created→OpenedLocalPendingDrain)` (edge 2, whitelisted `shifts.rs:78`), then the SHIFT_OPEN doc local-acks (OLA). Guard already permits it.
- **C10-quiescence over undrained OLA (the α risk) — RESOLVED**: `list_shift_pending_receipts_for_z_quiescence` (`fiscal_documents.rs:595`) EXCLUDES `OFFLINE_LOCAL_ACK` from the blocking set ("issued, not pending — what the aggregate counts"); `list_shift_issued_receipts` (`:566`) counts `state IN ('ACK','OFFLINE_LOCAL_ACK')`. ⇒ **an offline Z-close aggregates over its shift's undrained OLA docs by construction — no new aggregation code, no special bypass.**
- **No migration** (edges whitelisted, tables exist). DDL → STOP.

---

## 2. ⚖️ MAIN FORK α/β — edges 7/9 (architect ruling required — fiscal semantics)

| | **α — full offline close (WebCheck-parity)** | **β — online-close only (edge 2 only)** |
|---|---|---|
| Scope | edge 2 + edges 7/9 (`Opened`/`OpenedLocalPendingDrain → ClosingLocalPendingDrain`) + **lift the two guardrails** (`:1038`/`:1044`) RED-first vs their pins + CLPD→drain→Closed (edge 13, **reuse**) | edge 2 only; guardrails **stay**; offline close = refused |
| Offline Z | aggregates locally over the shift's OLA docs (**works by construction — C10 excludes OLA**) | n/a (close online after reconnect) |
| Operator story | open offline → sell offline → close offline (Z local) → drain later | open offline → sell offline → **reconnect** → drain (edge 5: OLPD→Opened) → online Z-close |
| Matches 4-yr prod | ✅ yes (WebCheck DocType=8/80 = ordinary offline docs) | ⚠️ partial — no all-day-offline close |
| Cost | edge 2 + 7/9 + guardrail-lift (+ pin inversion, consumer-completeness) + **2 drills** | edge 2 + **1 drill** |
| Risk | moderate; main risk (C10) **resolved** | low (no guardrail churn, no pin inversion) |
| Pilot workaround if absent | — | O2 runbook already says "close after reconnect" |

**My recommendation: β for the pilot** — it unblocks the O3 headline ("утро без сети" = edge 2) with the least hot-zone risk (no guardrail-lift, no pin inversion, 1 drill), is **coherent** (offline open + reconnect + online close), and is **consistent with the already-shipped O2 runbook** procedure. α's blocker is resolved and it's prod-faithful, but "close while still offline" is the rarer case and is workaround-covered; α is the clean follow-up if the pilot shows all-day-offline closes occur. **This is your fiscal-semantic call — I hold for your ruling.**

---

## 3. Sub-decision (needed in BOTH α and β — edge 2 is in both): offline SHIFT_OPEN numbering

Step-6 acquire_code is not doc-type-gated, so today a shift-doc would consume an offline code + `offline_fiscal_no`. WebCheck: shift-open does NOT increment the local check number. **Options:** (i) **skip Step-6 for shift-class** (WebCheck-faithful — shift-open takes no fiscal number) — requires the OLA invariant + drain to permit a shift-doc with NULL `offline_fiscal_no`; (ii) **consume a code** (simpler, uniform OLA/drain) — diverges from WebCheck numbering. Needs your fiscal-semantic call + I'll verify the OLA/drain invariants for (i) before wiring. INV-11/12 (a code must come from a real DPS-issued range) still apply to the SELLs regardless.

---

## 4. Explicitly NOT in scope O3 (named follow-ups)
Fuzzer-alphabet offline-shift-ops (separate Phase-3); T=112 ask-codes (campaign, INV-11 gap B6/B7); edge-11 reissue / ShiftRecoveryClass (recovery-increment).

---

**Awaiting:** (1) α/β ruling; (2) the 2b numbering sub-decision (i/ii). No code until ruled.
