# Fiscal Payment-Form Ledger — Design Dossier (WebCheck-mirrored)

**Status:** DRAFT for operator approval · **Date:** 2026-07-10 · **Branch:** `fuzzer-tier1-dossier`
**Origin:** operator directive — track per-form turnover + cash balance, enforce «готівка не може уходити в мінус», zero cash on auto-close; *«посмотри как в вебчеке… в Z-отчёте там всё есть»*.

## 0. Provenance & method

Reference model extracted from the DPS reference app **WebCheck PRRO32 v6.0.8.1368** (decompiled `docs/webcheck_reverse_v2/`). Three subagents read the decompile **fully and sequentially** (operator: «смотри подряд»), non-overlapping, then cross-validated:
- **B** — `ClassFiscal.cs` + delegates (`StringXML/Reports/All/SQLlite/CreateDB`): cash formula, the cash≥0 guards, Z-build XML, service-io, guard table.
- **C** — `FormPrint.cs` (7060 lines): printed Z/X/periodic structure.
- **A** — mid-size (`FormAccountant/FormCloseShift/FormReports/DateReport/TypPayReport/All/CreateDB`): periodic report, DB schema, close-flow, consolidated.

**The closing-cash formula was independently derived by all three agents from different files — high confidence.** Raw extractions: `scratchpad/webcheck_{A,B,C}_*.md`. All citations below are `docs/webcheck_reverse_v2/WebCheck/…` unless noted.

---

## 1. WebCheck reference model (SSOT)

### 1.1 Operational fact: cash/turnover/tax are DERIVED quantities (WebCheck = the NUMBER oracle, not our storage)
> **WebCheck is an OPERATIONAL oracle, NOT an architectural template** (operator pin, 2026-07-10). Take from it the fiscal BEHAVIOR — formulas, invariants, wire contract, error codes, Z sections, rounding. Do NOT copy its IMPLEMENTATION — storage, recovery, state machines, derive-vs-persist are OUR architecture, and we diverge where better (RMR > silent-retry; single-writer leases; **checkpoint+journal** > recompute-everything).

**Operational truth:** cash-on-hand, per-form turnover, and tax are **derived quantities** — functions of the primary SELL/RETURN/service events, never independent inputs. The cash number = the §1.2 formula. That is the operational rule we mirror (our reconcile must compute the SAME number WebCheck would).

*WebCheck's implementation (non-binding on us):* it recomputes on demand with no stored balance (`CreateDB.cs:217-256` — `SHIFTS` has no money columns; recompute over `CHECKPAY/…` by `SHIFTID`+`DOCTYPE`). **We deliberately diverge architecturally** (§5.1): persist a reconcilable checkpoint (the carried remainder). WebCheck's pure recompute is our *reconcile oracle* — the formula we check the checkpoint against — not our storage model.

**DocType taxonomy:** `0`=sell · `1`=return · `3`=service-in (внесення) · `4`=service-out (видача) · `-8`=EPZ cash-out · `8`=shift-open · `80`=Z · `9`=offline-begin · `10`=offline-end.

### 1.2 ★ Cash-on-hand formula (`All.Nal()`, `All.cs:431-463`)
```
cash_in_drawer = cash_sell − cash_return + service_in − service_out − epz_cash_out
```
- Backing aggregates: `Reprt2` (payform SMI/SMO, cash matched by literal name `"Готівка"`), `Reprt4` (service DocType 3→+ / 4→−), `EPZtoCash` (DocType −8).
- Per **single open shift**; `0.0` if none; **never persisted; never carried across shifts.**
- Printed as **`ГОТІВКА У СЕЙФІ`** on the Z/X (`FormPrint.cs:6482`), reconstructed from the Z XML sections (not a stored field).

### 1.3 ★★★ The «cash ≥ 0» invariant — the operator's rule, verbatim in the reference
**Four** cash-decreasing operations are refused **pre-mint** when `outgoing_cash > All.Nal()`, all raising **`errCode 47 "Помилка! У касі немає необхідної суми."`** The operator named all four:

| Operator's words | WebCheck site | file:line |
|---|---|---|
| «возврат за наличку» | SELL/RETURN cash-back-out (`opertyp>0 && Pay[1] > Nal()`) | `StringXML.cs:889-895` |
| «видача» | service видача `<O>` (`sumout > Nal()`) | `StringXML.cs:2620-2629` |
| «выдача картплательщику» | EPZ cash-to-cardholder (op −8) | `ClassFiscal.cs:1385-1392` |
| (решта) | change bounded by cash tender (`change > Pay[1]` → errCode 64) | `StringXML.cs:869-879` |

Floor = **exactly 0**; repeated withdrawals **compound** (`Nal()` nets prior out). No DB-level guard — pure fail-closed write-path validation before any row is minted. **Distinct from the offline code-reserve floor (T2).**

### 1.4 ★ Cash disposition at close = CONFIGURABLE {zero-on-close | carry-over} — operator pin «это ОЧЕНЬ важно»
`ClassFiscal.ReportZ:601-606`: `if shiftCashInOut=='1' → SumOut = Bablo(Nal()); <issue service видача of entire drawer>; then Z`. WebCheck implements **only ONE of two legitimate modes** — **zero-on-close** (drawer→0, next shift starts at 0, morning float via a fresh `службове внесення`). **The other mode — carry-over** (closing cash of shift N = opening cash of shift N+1, drawer never emptied) — is what **other POS systems use, and different POS interpret this differently** (operator). Therefore the gateway MUST support BOTH, **per-FN configurable** (compatibility priority #4 — the gateway must match whatever the driving POS expects). This is NOT an architectural pin toward «no carry»; it is a first-class config. Unified derive: see §5.7.

### 1.5 Z-report (DocType 80) = 6 sections (`StringXML.cs::ReportXZ:2140-2419`)
`<Z NO=sh>` → `<TXS>` per tax-group (SMI/SMO/TXI/TXO/DTI/DTO) → `<M>` per payform (T/NM/SMI/SMO[/SMIM/SMIP/SMOM/SMOP]) → `<NC NI NO>` counts → `<IO NM='ГОТІВКА' SMI SMO>` service cash → `<EPZ EPC EPSM>` → `</Z><TS>`. **Cash balance is NOT a wire field** — derived at print. Z-only footer: `ФІСКАЛЬНИЙ ЗВІТ ДІЙСНИЙ / РЕГІСТРИ ДЕННИХ ПІДСУМКІВ ОБНУЛЕНІ`.

### 1.6 X-report & Periodic
- **X** = same builder, header `X ЗВIT`, **no** offline-close/chain-advance/shift-close side-effects, **omits** the resetting footer. "Z right now without zeroing."
- **Periodic** (`DateReport.cs:48-111`) = **replays stored Z XML** over a date range (`SELECT checkxml FROM ksef WHERE dt∈[d1,d2] AND DocType=80`), sums each Z's `<M>`/`<TXS>`/`<NC>`. **Turnover+tax only — no cash/IO/EPZ.** Reads *stored Z* (not receipts) → survives the `CHECK*`-pruning trigger `ksefup8`.

### 1.7 Payform turnover, change, ISCASH mapping
- `Reprt2` (`Reports.cs:50`): `SUM(TOTALSUM) GROUP BY PAYMENTFORM` split DocType 0 (SMI)/1 (SMO), **parallel** to the per-tax-group aggregation.
- ★ **Change netted at write-time** (`StringXML.cs:1413`): stored cash `TOTALSUM = tendered − change` → turnover already excludes решта.
- **ISCASH → wire `T=`**: `1→"0"` (cash) · `102→"2"` (other) · else→`"1"` (cashless); `КАРТКА` special-cased to cashless. Cash also detected by name-synonym set.
- **PayForms ≤ 180**; first 4 locked (`Готівка/Картка/Кредит/Сертифікат`); no edits with an open shift.

### 1.8 Rounding
- `Bablo` = 2-dp banker's rounding on every money field.
- **VAT inclusive back-out** per group: `rate·gross/(100+rate)`; excise ГА `sum·5/105`; ПФ/military ДА `sum/1.275·0.075`.
- **НБУ cash 5-kopeck rounding** (`Okruglit5`, `StringXML.cs:1518-1541`): only when the total ends `.x0/.x5` and config on; **forbidden with any cashless leg** (→ errCode 102); delta emitted `SMP`/`SMM`.

### 1.9 Fail-closed input guards (the «валидаций мало» goldmine)
All pre-mint in `StringXML CheckProcessing→Product`:

| guard | condition | errCode |
|---|---|---|
| negative payment | `Pay[n] < 0` | 1017 |
| **cash cap (DopNal)** | `cash leg > AllowableCash` (default 50000, clamp 0..50000, per-FN, 0=off) — **per-receipt CASH leg, not total** | 70 |
| rounding w/ non-cash | cash-rounding + ≥2 forms | 102 |
| underpayment | `total > Σ payments` | 13 |
| change > cash | `change > cash tender` | 64 |
| duplicate payment id | ids repeat | 72 |
| multiple programmable rates | tax groups {4,5}&{6,7} both | 43 |
| invalid operationtype | not in {0,1,±8} | 19 |

Returns need **no** original in-shift (WebCheck `RETURN`=DocType 1, no lookup) — our `return_check_number` V1 guard is a safe **superset**.

---

## 2. Our current state (code reality)

| Capability | Status | Evidence |
|---|---|---|
| Payment forms modeled with `iscash` | ✅ have | `db/repositories/payment_methods.rs` (`PaymentMethod{pay_index,name,iscash}`; Готівка→`T="0"`) |
| Z `<TXS>` tax-group turnover | ✅ have | PR-Z1 `derive_z_report_tax_summaries`, `tax_summary.rs` |
| `calc_tax` VAT/excise SSOT | ✅ have | `xml/calc_tax` (matches 1.8 divisors) |
| STOP-S2 coupling tripwire | ✅ have | `z_builder.rs:30-58` — mandates building IO/EPZ Z-half if service-io/card ingress is relaxed |
| Z `<M>` per-payform turnover | ✅ **HAVE** (self-check corrected) | `aggregate_zreport` (`convert.rs:410`) groups payments `(type_code,name)`→SMI/SMO, emitted as `<M>` in `<Z>` (`xml/mod.rs:17,166`); unit-tested (`convert.rs:1103-1205`). **The operator's «оборот по формам» is already satisfied for the Z.** |
| Z `<NC>` counters | ✅ HAVE | `aggregate_zreport` sell_count/return_count (`convert.rs:413-423`) |
| Running cash balance (derive) + `cash_balance_kop` | ❌ **gap (logic)**; column exists but DEAD | `shifts.cash_balance_kop` inserted as literal `0` (`shifts.rs:136`), **never UPDATEd, never read for a guard** (opened_at pattern). No derive logic. L0 choice: derive-on-read (ignore/repurpose the dead col) vs revive-and-maintain it. |
| **cash ≥ 0 invariant** | ❌ **gap (highest value)** | no cash-floor guard anywhere |
| Service in/out (DocType 3/4) on wire | ❌ gap | `ServiceIn/ServiceOut` are **fail-closed** at `build_send_envelope` + refused at `policy.rs` (422) |
| Z `<IO>` / `<EPZ>` sections | ❌ absent-by-guard | STOP-S2 — legitimately absent only because ingress refuses the inputs |
| Zeroing auto-видача at close | ❌ gap | needs ServiceOut wired + balance |
| X-report / Periodic report | ❌ gap | `XReport` fail-closed; no periodic |
| Fail-closed input guards (§1.9) | ❌ mostly gap | only `return_check_number` V1 exists |
| 50k cash cap | ❌ gap | (gap #5 in `project_fiscal_correctness_gaps`) |

**Good news on scope:** the cash-balance ledger builds on data we already have (sales carry `<M>`/`iscash`); only **service-io** must be newly wired, and **card-EPZ stays closed** (zeroing doesn't need it). Clean cut.

---

## 3. New legal invariant (for `docs/LEGAL_INVARIANTS.md`)

**INV-21 (cash ≥ 0):** the derived cash-on-hand for the open shift MUST NEVER go below zero. Any cash-decreasing operation — cash return, service видача, EPZ cash-out, or change exceeding the cash tender — that would drive `cash_in_drawer < 0` is **refused fail-closed, row-less, pre-inbox** (analog of WebCheck errCode 47). Cash-on-hand = `cash_sell − cash_return + service_in − service_out − epz_cash_out`, per open shift, recompute-on-read. Distinct from INV-05 (offline code/time limits) and the T2 code-reserve floor.

---

## 4. Decomposition into increments (RED-first each)

Ordered by value-density and dependency. Each lands test-first with a stated RED pin + teeth + a mandatory **Fuzzer-impact** section (per `feedback_fuzzer_tracks_features`).

- **L0 — Cash-ledger: carried-remainder anchor + bounded per-shift derive (operator 2026-07-10).** `cash_on_hand = opening_cash(anchor persisted at shift-open) + Σ(this shift's cash movements)`; at close `closing_cash` persisted → next open's anchor (carry-over default). **Revives the dead `shifts.cash_balance_kop` column** as the opening anchor (no migration). Cash-leg classification reused from `aggregate_zreport`. **★ Reconcile-on-boot** re-derives the anchor from the journal and audits drift (journal = SSOT; checkpoint honest — invariant #8). Bounded derive (O(shift), not O(all-history)) — the reason to persist vs pure-derive. *RED:* carry across shifts (B opens at A's close) + boot-reconcile detects a corrupted anchor.
- **L1 — ★ cash ≥ 0 invariant (INV-21).** Highest value, smallest blast. Pre-inbox fail-closed refusal on cash-return / (later) видача / (later) EPZ-out / change-over-cash when `out > L0.balance`. *RED:* cash return exceeding drawer → row-less refuse; teeth = revert guard → RED. Depends L0. **Can land early — before service-io wiring** (the cash-return + change sites need no new doc types).
- **~~L2 — Per-payform turnover in Z (`<M>`)~~ — ALREADY DONE (self-check).** `aggregate_zreport` already builds `<M>` SMI/SMO per `(type_code,name)` and wires it into `<Z>`, unit-tested. Residual (small, fold into L1/L3): confirm the `iscash`→`T=` classification (1→"0"/102→"2"/else→"1") + `≤180` guard match §1.7. **No standalone increment needed.**
- **L3 — Service in/out wiring (DocType 3/4) + Z `<IO>` — same change (STOP-S2).** Relax `policy.rs`/`build_send_envelope` for `ServiceIn/ServiceOut` (`<C T='2'>` with `<I>`/`<O>`), AND build the Z `<IO>` aggregation in the SAME PR (the coupling-pin mandate), AND flip `FULL_Z_SURFACE_READY` reasoning. Extends L1's cash≥0 to the видача site. **Card-EPZ stays refused** (guard closed). *RED:* видача > drawer → refuse (INV-21); a внесення then видача nets in the Z `<IO>`.
- **L4 — Cash disposition at close (per-FN config {zero | carry}).** On any close (manual or I-1 auto-close): if `cash_disposition==zero` → issue a service видача of `L0.balance` before the Z (= WebCheck `shiftCashInOut`; drawer→0; next shift from 0); if `carry` → no видача, closing balance becomes the next shift's opening (§5.7). Zero-mode fallback: if видача fails, close anyway un-zeroed + audit. **Threads into T2:** offline close-reserve `+1` code only when `zero-mode && balance>0`. Depends L0+L3. *RED:* zero-mode over-limit shift w/ cash → видача=balance then Z, drawer=0; carry-mode → no видача, next shift opens at prior close.
- **L5 — Fail-closed input guards (§1.9).** negative-amount · **50k cash-leg cap = HARDCODED `50_000_00` kop pre-pilot** (operator: «все до пилота 50000 хардкод»; per-FN config deferred post-pilot) · underpayment · change>cash · duplicate-payment-id · single-programmable-rate · operationtype-whitelist. Independent, closes the «валидаций мало» gap + fiscal-gap #5. *RED:* one pin per guard, each a pre-inbox 422. (Boundary to confirm vs legal text: WebCheck refuses cash-leg `> 50000` i.e. allows 50000.00; operator gap #5 noted «макс 49999.99» — pick `>` vs `≥`.)
- **L6 — X-report.** Reuse the Z builder; gate all resetting/closing/side-effects to the Z path; no footer. *RED:* X mid-shift = current turnover, shift stays open, no chain advance.
- **L7 — Periodic report.** Replay/sum stored Z documents over a date range (turnover+tax only). Pruning-resilient (read stored Z, not receipts). *RED:* range of 2 Z's → summed `<M>`/`<TXS>`.

**Parallel, independent of the ledger — already scoped (previous turn):**
- **I-1 — Shift-limit timing + block.** Default hard-block at limit; **opt-in preventive auto-close at `limit − 5min`** (pin: `margin ≥ ticker_interval`, else coarse ticking fires late — the «смысл теряется» failure); configurable scheduled-time close (HH:MM Kyiv, next occurrence after open, capped by the margin close); POS-status surface + a POS-actionable block-refusal reason. Revises RULING-3's «unconditional» → config-gated. T3 code per- weighted, not discarded. L4 (zeroing) fires from I-1's auto-close when both enabled.

---

## 5. Architectural pins

1. **Checkpoint + journal (operator decision) — bounded, reconcilable, no migration.** Cash is a **carried remainder** persisted at shift open/close (revive the dead `shifts.cash_balance_kop`), with `cash_on_hand = opening_anchor + Σ(this shift's cash movements)`. The anchor is a **checkpoint**, the journal (`fiscal_documents`) remains SSOT — a reconcile-on-boot re-derives and audits drift (so it can never silently diverge, unlike a blind counter; preserves invariant #8). Chosen over WebCheck's pure per-shift derive because carry-over mode needs a bounded opening anchor (WebCheck avoids this only by zeroing every shift). No new migration (column exists). WebCheck's pure derive (1.1) remains the reconcile oracle.
2. **Per-payform ledger runs PARALLEL to per-tax-group** — two independent aggregations; a tax-only Z (today) structurally misses `<M>`.
3. **STOP-S2 is law:** wiring `ServiceIn/Out` (L3) MUST build the Z `<IO>` half in the same change, or the Z under-reports. Card-EPZ guard stays closed until its EPZ Z-half is built (out of this dossier's scope).
4. **cash≥0 (INV-21) is pre-inbox, row-less, floor=0**, distinct from the T2 code-reserve floor and INV-05 time/code limits.
5. **Change is netted at receipt-write** (turnover excludes решта) — do NOT double-count.
6. **T2 interaction:** L4 zeroing adds one offline code when `balance>0` → `required_codes_to_close += (zeroing && balance>0 ? 1 : 0)`.
7. **★ Cash disposition at close is CONFIGURABLE {zero-on-close | carry-over}, per-FN (operator: «ОЧЕНЬ важно»; different POS interpret differently).** Unify both under one derive: `cash_on_hand = Σ(cash movements since the last "zero-point")`, where a zero-point = a full zeroing видача OR an explicit opening-cash/inventory anchor.
   - **zero-on-close** (WebCheck): close emits a zeroing видача → every close IS a zero-point → each shift derives from 0 (pure per-shift, no stored state).
   - **carry-over**: close emits NO zeroing → no zero-point at close → the derive sums across shifts back to the last real zeroing/inventory anchor; opening cash of shift N = closing of shift N−1, all still derive-on-read (the anchor is itself a document — no balance column).
   The config only decides **whether close emits a zeroing видача**; the derive function is identical. Preserves derive-on-read in both modes. Must MATCH the driving POS's expectation (compatibility #4).

---

## 6. Fuzzer-impact (mandatory — `feedback_fuzzer_tracks_features`)

The ledger adds new state + invariants → the invariant fuzzer's alphabet must grow (feeds RAGE Tier-2 / `FUZZER_TIER2_RAGE_DOSSIER.md`):
- **New ops:** `Op::ServiceIn`, `Op::ServiceOut`, `Op::EPZCashOut` (and cash-vs-cashless SELL/RETURN variants — the model currently treats payment abstractly).
- **New model oracle:** a per-shift cash-balance accumulator = §1.2 formula; predict **INV-21 refusals** independently (spec, not prod helper) → differential vs prod; teeth = revert the cash-floor guard → RED.
- **Per-form turnover oracle:** model predicts `<M>` SMI/SMO per form; differential vs the Z builder.
- **Known-red until built:** cash-io ops are OUT of the current alphabet — fence as known-red, not silent-absent.
This is a first-class RAGE wave (`W-ledger`), gating nothing until L1 lands but tracked from now.

---

## 7. Open questions for the operator

1. **Increment order** — land **L0+L1 (cash≥0) first** (highest value, no new doc types, closes the core invariant), then L2/L3 (payform Z + service-io), then L4 (zeroing)? Or bundle L0-L4 as one «ledger» push?
2. **Cash disposition {zero | carry}** — RESOLVED: per-FN configurable, both modes; **default = carry-over** (operator, 2026-07-10 — safer: never asserts an unphysical cash movement; zero-on-close is opt-in for cash-to-safe shops).
3. **50k cash cap (L5)** — RESOLVED: **hardcoded `50000` pre-pilot** (operator), per-FN config post-pilot. (Confirm `>` vs `≥` boundary vs legal text.)
4. **X + periodic (L6/L7)** — pilot-gating or post-pilot? (They're operator-facing reporting, not correctness-critical for the write-path.)
5. Anything WebCheck does that you want us to **deliberately diverge** from (e.g. we already keep RMR where WebCheck silently retries; we could add a durable balance cache WebCheck lacks)?

---

## 8. Roadmap position

This dossier sits alongside the offline-limit line (T1/T2/T3 done) and the RAGE fuzzer waves. Suggested sequence: **I-1 (timing+block)** and **L0+L1 (cash≥0)** are the two near-term, independent, high-value increments; L2-L4 form the «payform Z + zeroing» middle; L5 (guards) is independently landable anytime; L6/L7 (reports) are operator-facing and can trail. Live-campaign readiness is unaffected by L6/L7 but **benefits from L1+L5** (correctness hardening) before pilot.

---

## 9. Self-verification log (operator: «проверь сам сильно», 2026-07-10)

Load-bearing claims re-read directly against source (not trusting subagent reports):

**WebCheck reference — ALL verified VERBATIM (subagents accurate):**
- ✅ `Nal()` cash formula — `All.cs:462`: `num − num2 + (num3 − num4) − num5` = cashSell−cashReturn+svcIn−svcOut−epzOut; cash payform matched by literal `"Готівка"` (`:451`).
- ✅ cash≥0 guard 3a (SELL/RETURN cash-out) — `StringXML.cs:889-895`: `if opertyp>0 && Pay[1] > Nal() → errCode 47 "Помилка! У касі немає необхідної суми."` verbatim.
- ✅ cash≥0 guard 3b (service видача) — `StringXML.cs:2620-2629`: `<O>` emitted only if `sumout ≤ Nal()`, else errCode 47. verbatim.
- ✅ cash≥0 guard 3c (EPZ −8) — `ClassFiscal.cs:1385-1392`: `if amt > Nal() → errCode 47`; EPZ doc `operationtype='-8'` taxrate='2' verbatim.
- ✅ Zeroing at close — `ClassFiscal.cs:601-606`: `if shiftCashInOut=='1' → SumOut=Bablo(Nal()); CashInOut()` (full-drawer видача before Z), config-gated. verbatim.
- ✅ 50k cap — `All.cs:880-886`: clamp `<0→0`, `>50000→50000`; per-FN INI `AllowableCash`.

**Our code — 2 corrections, BOTH shrink the gap:**
- ✅→ **`<M>` per-payform turnover + `<NC>` counters ALREADY BUILT & WIRED** (`aggregate_zreport` `convert.rs:410`, `<M>` in `<Z>` `xml/mod.rs:17,166`, unit-tested `:1103-1205`). Was mislabeled "verify/gap" → **L2 dropped** (operator's «оборот по формам» already met for the Z).
- ⚠️ **`cash_balance_kop` is a DEAD column** = literal `0` at open (`shifts.rs:136`), no UPDATE, no guard-read → gap is the LOGIC (derive + cash≥0), not a missing column.
- ✅ ServiceIn/ServiceOut/CashWithdrawal/PeriodicReport confirmed `Unsupported` at ingress (`policy.rs:56-59`) + fail-closed at `build_send_envelope`.

**Net:** reference model is solid (verbatim-confirmed); real remaining gaps = running cash-balance LOGIC + **★cash≥0 guard (INV-21, L1)** + service-io wiring & Z `<IO>` (L3) + zeroing/carry disposition (L4) + fail-closed input guards (L5) + X/periodic (L6/L7). Turnover ledger (L2) already exists.
