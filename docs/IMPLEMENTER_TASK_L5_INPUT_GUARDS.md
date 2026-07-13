# Implementer Task — L5 fail-closed input guards (pre-pilot hardening)

**Base:** branch from `epz-fuzzer-fullalpha` (STACKED on EPZ — L5 and EPZ both touch
`convert.rs`; stacking avoids conflict and keeps the batch linear). New worktree
`l5-input-guards`. **LOCAL ONLY — commit per slice, do NOT push** (operator: push later).
**Method:** strict RED-first TDD. Mirror the existing pre-inbox row-less 422 guard pattern
(`ConvertError` in `convert.rs`, mapped in `handler.rs`).

## Context (from L5 research, verified on 9062550)
`convert.rs` already has 17 fail-closed pre-inbox guards (`EmptyGoods`, `ZeroQuantityLine`,
`CashInsufficient` INV-21, `EpzPaymentIdTooLow`, …), all → 422 via
`convert_error_code`→`http_status_for_error_code`. L5 adds 4 MISSING guards in the same
`CommandType::Sell | CommandType::Return` arm (`convert.rs:~875-922`), mirroring the INV-21
guard shape. Structurally-already-guarded (DO NOT re-add): negative amounts (u64 wire types),
unknown doc_type (serde enum), duplicate idem key (inbox `Replay`/`Conflict`).

## Decided scope (4 guards — do not widen)

| # | Guard | New `ConvertError` variant | errCode string | HTTP |
|---|---|---|---|---|
| G1 | cash legs (type_code=="0") Σ **> 4_999_999 kop** (49 999.99 UAH) | `CashCapExceeded { cash_kop, cap_kop }` | `CASH_CAP_EXCEEDED` | 422 |
| G2 | a good with `item_sum_kop == 0` (zero price) | `ZeroPriceLine { item_name }` | `ZERO_PRICE_LINE` | 422 |
| G3 | a payment with `sum_kop == 0` | `ZeroPaymentAmount { pay_index }` | `ZERO_PAYMENT_AMOUNT` | 422 |
| G4 | **SELL** Σpayments < Σgoods (underpayment) | `UnderpaymentRefused { goods_kop, paid_kop }` | `UNDERPAYMENT_REFUSED` | 422 |

**Decisions (locked, no re-litigation):**
- **G1** caps the CASH portion (WebCheck `DopNal/AllowableCash` semantics, `All.cs:875-886` clamps
  `DopNal` cash cap to ≤50000), NOT the receipt total. Hardcode `4_999_999` kop (pre-pilot).
- **G4** is SELL-only (RETURN is a refund — underpayment semantics don't apply). Moves the
  cross-check that currently lives POST-inbox in `stage_sign` to a PRE-inbox row-less refusal.
  Keep the stage_sign check too (defense-in-depth); L5 just adds the earlier fail-closed gate.
- **G2/G3** fire per-line / per-payment inside the existing item/payment loops.
- **DPS errCode analog:** grep `docs/webcheck_reverse_v2/FRESH_WEBCHECK_ANALYSIS.md` +
  `All.cs` error table for the closest analog to log alongside each (over-cap / zero /
  underpayment); if none clean, log the descriptive string. NOT blocking — the 422 is the gate.
- **Deferred (out of L5, log as known-not-done):** change>cash (verify the wire `Totals`
  carries a change/rest field first — GAP), tax-group range check (stage_sign `calc_tax`
  already fails on invalid TXAL — lower priority).

## V0 — RED-first pins (TEST-ONLY, write + watch fail FIRST)
1. **G1 pin:** SELL with a single cash payment of 5_000_000 kop → 422 `CASH_CAP_EXCEEDED`,
   row-less (no doc minted). Boundary: 4_999_999 kop → PASS; 5_000_000 → REFUSE.
2. **G2 pin:** SELL with a good `price_kopecks=0` → 422 `ZERO_PRICE_LINE`, row-less.
3. **G3 pin:** SELL with a payment `amount_kopecks=0` → 422 `ZERO_PAYMENT_AMOUNT`, row-less.
4. **G4 pin:** SELL goods total 1000 kop, payments total 900 kop → 422 `UNDERPAYMENT_REFUSED`,
   row-less. Exact-match (1000==1000) → PASS.

## V1 — implement (minimal diff)
`convert.rs`: 4 `ConvertError` variants + the 4 checks in the `Sell | Return` arm (G1/G4
SELL-scoped; G2/G3 both). `handler.rs`: 4 arms in `convert_error_code` + 4 codes in the 422
bucket of `http_status_for_error_code`. No schema/migration (input guards only).

## V2 — fuzzer (MANDATORY — real teeth, #257 lesson)
The current fuzzer amount generation likely uses bounded/fixed amounts that NEVER hit these
guards → the guards would be DEAD from the fuzzer's view (fake-safe). Add a dedicated amount
strategy so the fuzzer EXERCISES each guard:
- extend the SELL op's amount generation with `prop_oneof![valid, over_cap (≥5_000_000),
  zero_price, zero_payment, underpaid]` (or an equivalent bounded strategy spanning the
  guard boundaries), so generated sequences reach each L5 refusal.
- model: an over-cap / zero / underpaid SELL → `ExpectedOutcome::NoMutation` (row-less refuse)
  — the model must predict the refusal (independent of prod), else it's fake teeth.
- **Teeth (prove empirically — revert→RED→restore, run yourself):** for EACH of G1–G4,
  reverting the prod guard must make a seeded harness driving that violation go RED (prod mints
  a row the model says shouldn't exist → `ExpectedNoMutation` assertion fires). Include ≥1
  seeded harness per guard (or one harness that drives all four).

## Invariants — HOLD (state each preserved)
- Pre-inbox row-less: no doc minted on refusal (audit_log only) — same class as EmptyGoods.
- Idempotency, z-quiescence, INV-21, advance-at-SEND — untouched (input-shape guards only).
- No network/crypto in txn (pure in-memory checks pre-inbox).
- INV-6 unaffected (guards reject malformed input, don't summarize payload).

## Verification gate (run YOURSELF on the FINAL commit — actual numbers)
`cargo nextest run -p prro --features test-support` (0 failed) + `cargo fmt -p prro -- --check`
+ `cargo clippy -p prro --all-targets --features test-support -- -D warnings` (0). Plus the
G1–G4 teeth-proofs (revert each → RED → restore).

## Delivery (required 7) + commit per slice (V0→V1→V2). Do NOT push/merge — architect
verifies + reviews; the batch pushes later on operator command.
