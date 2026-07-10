# Implementer contract — L0 + L1: cash ledger + готівка≥0 (INV-21) + fuzzer

**Reads with:** `docs/FISCAL_PAYFORM_LEDGER_DOSSIER.md` (§1 reference model verbatim-verified, §9 self-check). **Discipline:** strict RED-first TDD — every task's FIRST commit is a failing test you watched fail. **TEST-ONLY code is forbidden in prod files.** Minimal diff; hot zones = `write_path` + `ingress` → preserve frozen invariants (esp. #1 no net/crypto in write tx, #2 single-writer, #4 idempotency, #8 recovery-safe transitions).

## 0. Resolved decisions (operator, 2026-07-10)
- **Cash disposition default = carry-over** (per-FN configurable later; carry = cash accumulates across shifts, no zeroing видача). L0/L1 implement the derive under carry semantics; the zeroing action is L4 (out of scope here).
- **50k cash cap boundary = `< 50000`** (refuse cash-leg `≥ 50_000_00` kop; max 49999.99) — that guard is L5, NOT this task; noted for consistency.
- **Fuzzer extension is MANDATORY** in this task (operator: «не забудь добавить поведение к фазеру»).

## 1. Scope
**IN:** L0 derived cash-on-hand (recompute-on-read, pure) + L1 the готівка≥0 invariant on the ONE cash-decreasing op wireable today — a **cash RETURN** — refused pre-inbox row-less. Fuzzer alphabet + oracle + teeth for both.
**OUT (later increments — all PILOT-SCOPE per real-data evidence, each attaches the identical INV-21 check):**
- **L3 — ServiceOut/ServiceIn (DocType 4/3 видача/внесення) + Z `<IO>` section (STOP-S2)** — REQUIRED before pilot (hundreds of видача per FN in real dumps, ≈once/shift). Carries: the explicit fiscal видача (guard-3b), the `auto_zero_at_close`/`shiftCashInOut` zeroing видача action, and the служ. внесення (morning float).
- **«выдача ДС по карте» / EPZ cash-out (op −8) + Z `<EPZ>`** (operator: «на пилоте есть») — guard-3c.
- change>cash-leg + 50k + other input guards (L5); X/periodic (L6/L7).

**In L0 (this task):** only the DERIVE + CARRY (default). Do NOT wire ServiceIn/Out/EPZ here (their formula terms stay 0). The `auto_zero_at_close` config may be parsed (default off) but its fiscal видача is L3 — do NOT emit a document-less zero.

---

## 2. L0 — cash-on-hand ledger: carried-remainder anchor + bounded per-shift derive

**Operator decision (2026-07-10): cash is a carried remainder (переходящий остаток) persisted at shift open/close — NOT a pure all-history derive.** Revives the dead `shifts.cash_balance_kop` column (no migration). Journal stays SSOT; the anchor is a reconcilable checkpoint.

**★ Zeroing the drawer at close = a REAL FISCAL видача (proven empirically 2026-07-10).** In real WebCheck dumps (`/home/setter/webcheck_dumps/*.db`), every service видача (DocType 4) carries a DPS fiscal number (`checkidficscal`), `offline=0`, firing ~once/day at close (= `shiftCashInOut`). **Operational truth: you cannot zero cash without a fiscal document.** So «в 0 при закрытии» needs the ServiceOut wire → **L3**, NOT L0. (An earlier "bookkeeping-only zero" framing was withdrawn — data refuted it.)

**L0 implements the DERIVE + CARRY (default) only:**
```
during:   cash_on_hand = opening_cash + Σcash(SELL) − Σcash(RETURN) + Σ(service-in) − Σ(service-out) − Σ(EPZ-out)
          ↑ FULL §1.2 formula — write ALL five terms now; service-in/out & EPZ-out are 0 until wired (L3) → forward-compatible.
at OPEN:  opening_cash = prior shift's closing_cash   (carry — default, fully in L0)   | 0 (first shift)
at CLOSE: closing_cash = cash_on_hand                 (persisted checkpoint)
```
- **CARRY (default) — fully in L0** (opening = prior close). The reconcile zero-point in pure carry = FN-inception / the last real видача.
- **`auto_zero_at_close` config (= `shiftCashInOut`)** may be INTRODUCED (parsed, default off) in L0, but its ACTION — auto-emitting the zeroing видача at close — is a **fiscal ServiceOut document → lands in L3**. Until L3, zero-config is inert (carry behaviour) + an audit note; do NOT fake a document-less zero.
- **ServiceOut (DocType 4 видача) is PILOT-SCOPE** (hundreds/FN in real data, ≈once/shift) — the L3 increment is required before pilot, not deferred.
- WebCheck's `shiftCashInOut` is the operational proof that zeroing is fiscal; whether WE auto-issue at close or require an explicit operator видача is our UX choice (both are the same fiscal ServiceOut doc, L3).
- **Persist** `opening_cash` on the shift row at OPEN **in the same tx as the shift-open transition** (atomic — invariants #2/#8). Revive `shifts.cash_balance_kop` (currently literal `0`, `shifts.rs:136`) as the opening anchor; the open INSERT that today binds `0` must instead bind the prior shift's closing balance (carry). **No new migration.**
- **Cash-leg** = payment(s) with **stored `type_code == "0"`** — reliably cash by the **D1 frozen-slot invariant** (verified: `CASH_SLOT=1` `preflight.rs:23` → cash always pay_index 1 → type_code "0" `convert.rs:701`; admin refuses to break slot-1-cash `admin_w4_z0.rs:108-164`; receipt-conversion validates `iscash==kind_is_cash` `convert.rs:730`; boot-preflight checks D1 `supervisor.rs:374`). **This supersedes the earlier "use iscash not position" note** — a `payment_methods.iscash` lookup would need `secure_pool` at the hot close/derive path (unavailable in `run_one_attempt`), whereas `type_code=="0"` makes the derive a **self-contained MAIN-pool query over `fiscal_documents`** — no secure_pool, no signature churn. Add a comment: «cash = type_code "0" per D1 frozen invariant». Sum the **stored `sum_kop` directly** — it is already the accounting amount; **`RM` (change) is informative-only, do NOT subtract** (operator). Reuse `aggregate_zreport` (`convert.rs:410`) — take its `type_code=="0"` row — so the balance is ONE aggregation, cannot diverge from the Z `<M>`.
- **Bounded per-shift derive** — the `+ Σ(this shift's movements)` part is a pure function over the shift's docs (mirror `tax_summary.rs`/`aggregate_zreport`, unit-testable without a DB); the opening anchor is read from the shift row.
- **★ Reconcile-on-boot (drift teeth) — REUSE the existing boot recovery / `invariant_scan` seam, do NOT add a new startup pass (opt #4):** re-derive the anchor from the journal (Σ cash movements since the last zero-point) and compare to the stored `opening_cash`; mismatch → audit/alert (journal is authoritative). Keeps the checkpoint honest — can never silently drift. Recovery-safe (invariant #8).
- **Zero-point seam:** the "since last zero-point" is FN-inception today (carry, no zeroing видача exists until L4). Leave a one-line seam so L4's zeroing видача becomes the anchor; do NOT build L4.

**RED pins (write first, watch fail):**
1. `pin_l0_empty_shift_zero` — first shift, no docs → `cash_on_hand == 0` (opening 0).
2. `pin_l0_cash_sales_sum` — two cash SELLs (100.00 + 25.00) → `12500` kop.
3. `pin_l0_cash_return_subtracts` — cash SELL 100.00 then cash RETURN 30.00 → `7000` kop.
4. `pin_l0_noncash_excluded` — a card (`T="1"`) SELL 50.00 does NOT change cash-on-hand.
5. `pin_l0_mixed_receipt` — one receipt cash 40.00 + card 60.00 → cash-on-hand += `4000` only.
6. **★ `pin_l0_carry_across_shifts`** — shift A: cash SELL 100.00, close (closing 100.00); shift B opens → `opening_cash == 10000`; a cash SELL 20.00 in B → `cash_on_hand == 12000` (carry proven).
7. **★ `pin_l0_boot_reconcile_detects_drift`** — corrupt the stored anchor, run reconcile → mismatch detected/audited, re-derived value wins (teeth against drift).

---

## 3. L1 — INV-21 «готівка ≥ 0» pre-inbox guard on cash RETURN

**Invariant (new — add to `docs/LEGAL_INVARIANTS.md` as INV-21):** the derived cash-on-hand MUST NEVER go below zero. The SAME check — «перед выдачей проверка, что денег хватило»: `amount_out > cash_on_hand(fn) → refuse fail-closed, row-less, pre-inbox` (no fiscal_documents row minted), analog of WebCheck errCode 47 `"Помилка! У касі немає необхідної суми"` — is enforced at **all three cash-out sites** (operator, verified verbatim = WebCheck's three guards):

| Cash-out site | WebCheck guard | Enforced in |
|---|---|---|
| **возврат за нал** — a cash RETURN | 3a `StringXML.cs:889` | **L1 — THIS task** (RETURN exists today) |
| **выдача** — service видача (ServiceOut) | 3b `StringXML.cs:2620` | L3 (op is fail-closed today) |
| **выдача ДС по карте** — EPZ cash-to-cardholder | 3c `ClassFiscal.cs:1385` | L3+ (op does not exist today) |

The invariant is DEFINED once (here) and applied at each site as that operation gets wired. **This task wires site 1 (cash RETURN)** — the only cash-out op that exists today. Sites 2+3 attach the identical check when L3 wires ServiceOut/EPZ (a `debug_assert`/known-red fence marks them pending, per §4.5).

**Placement:** mirror the existing **`return_check_number` V1 pre-inbox 422 guard** (RETURN+ line, STOP-R1) — same ingress validation seam, same row-less refusal shape. Use a distinct refusal code (e.g. `CASH_INSUFFICIENT` / errCode-47 analog); return the same class of pre-inbox rejection (HTTP 422 for REST) with the deficit surfaced.

**Co-locate (opt #5, if cheap at the same seam):** while here, add the two trivial-but-high-value pre-inbox guards from §1.9 — **negative-amount** (errCode-1017 analog) and **operationtype-whitelist** {SELL,RETURN,…} (errCode-19 analog). Same validation pass, one extra pin each. (The heavier L5 guards — 50k cash-cap, underpayment, change>cash, dup-id, single-programmable-rate — stay in L5, NOT here.) Do not let this balloon L1; skip if it complicates the seam.

**RED pins + teeth (write first):**
1. `pin_l1_return_over_empty_drawer_refused` — open shift, no cash sales (drawer 0), submit cash RETURN 1.00 → **row-less refuse** (assert: 422/refusal code, NO fiscal_documents row, NO inbox row).
2. `pin_l1_return_within_balance_ok` — cash SELL 100.00, then cash RETURN 50.00 → accepted; then cash RETURN 60.00 → refused (drawer now 50.00 < 60.00).
3. `pin_l1_noncash_return_not_gated` — a CARD return does NOT consult cash-on-hand (only the cash leg is gated); it proceeds (no cash-floor refusal).
4. `pin_l1_exact_zero_ok` — drawer 50.00, cash RETURN 50.00 → accepted (result 0, not < 0; floor is inclusive-0).
5. **★ teeth** `pin_l1_teeth_revert_guard` — a canary asserting that with the guard removed, pin 1's RETURN would mint; keep as a documented revert-proof (the fuzzer differential below is the durable teeth).

**Invariant-preservation notes to state in the PR:** the guard is pre-inbox (no write-tx, no network) → invariants #1/#2 intact; row-less refusal is idempotent (re-submit → same refusal) → #4; no state transition is created for a refused doc → #8.

---

## 4. Fuzzer extension (MANDATORY — operator)

Extend the invariant differential fuzzer (`tests/invariant_fuzzer/`) so the ledger + INV-21 are model-checked with teeth (feeds `FUZZER_TIER2_RAGE_DOSSIER.md`):

1. **Alphabet:** ensure SELL/RETURN ops carry a **cash-vs-noncash payment** dimension (per `project_fuzzer_alphabet_gaps` the model treats payment abstractly — add the cash flag). No new shift ops needed for L0/L1.
2. **Model oracle (INDEPENDENT spec, not a prod helper):** a per-FN cash accumulator = §1.2 restricted (SELL cash `+`, RETURN cash `−`), carry-over. Predict INV-21 refusal iff a cash RETURN would take the accumulator `< 0`.
3. **Differential:** run the op sequence against prod; assert prod refuses exactly when the model predicts, and the prod cash-on-hand (L0) equals the model accumulator after every step.
4. **★ Teeth (durable):** reverting the L1 prod guard MUST make the differential go RED (the model still predicts refusal; prod now mints → mismatch). State this teeth explicitly.
5. Fence anything not yet built (service-io/EPZ cash-out sites) as **known-red**, not silent-absent.
6. **★ Coordinate with RAGE W1 (opt #10, task #18 — also editing `tests/invariant_fuzzer/model.rs` for shift ops):** grow the model ONCE, coherently — align the cash/payment-state additions here with W1's shift-op additions so the two efforts don't push `model.rs` in conflicting directions (merge-pain + alphabet drift). If W1 is mid-flight, rebase onto / align with its model shape; note the coordination in the PR.

---

## 5. Delivery (per CLAUDE.md 7-point format)
Intent · Files changed · Tests/checks run (gate green + the RED→GREEN transitions you watched) · Result · Known risks/not-done (L3/L4/L5 sites deferred) · **Invariant check** (INV-21 added; frozen #1/#2/#4/#8 preserved) · Suggested next step (L3 service-io + Z `<IO>`). Work in a worktree off `main`; branch `feat/l0-l1-cash-ledger`; do NOT push to main; open a PR for review.
