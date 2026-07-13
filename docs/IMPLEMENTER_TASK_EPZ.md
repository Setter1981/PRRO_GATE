# Implementer Task — EPZ (видача готівки за ЕПЗ / cash advance), bimodal

**Design source (authoritative):** `docs/EPZ_DOSSIER.md` — read it fully first.
**Base:** branch from `origin/main` (`9062550`) in an ISOLATED WORKTREE. Do NOT use the
current working tree (`fuzzer-tier1-dossier` is stale, pre-L3).
**Method:** strict RED-first TDD. Every production change preceded by a failing test.
**Two hard mandates from the operator:** (1) **HOLD THE INVARIANTS** (§Invariants below —
state each as preserved); (2) **FINISH THE FUZZER** — EPZ must be a first-class fuzzer op,
GREEN, not known-red, with teeth.

---

## Decided design calls (do not re-litigate)

1. **DocType:** NEW variant `CashAdvanceEpz => "CASH_ADVANCE_EPZ"` (do NOT reuse the
   fail-closed `CashWithdrawal` placeholder).
2. **Z `<EPZ>` `EPCS`:** hardcode `0` (byte-parity with WebCheck `FormDate.cs:436`).
3. **`paymentid ≥ 2`** inline fail-closed guard (errCode-94 analog, 422) — card form only.
4. **`taxrate` on `<goods>`:** WebCheck uses `'2'`. **VERIFY against our DPS tax-code table
   (`docs/webcheck_reverse_v2/` tax mapping) before hardcoding — do NOT guess.** Pin it
   with a wire test once confirmed.
5. **Card requisites:** reuse the existing `AcquirerSlip` (`dto.rs:301`) / `CheckPayment`
   EPZ attrs (`xml/mod.rs:642-663`, PA/PB/PC/PD/PE/PF/PSNM/RRN). No new adapter plumbing.

---

## Authoritative wire (from `ClassFiscal.EPZtoCash`, dossier §2) — pin byte-exact

⚠ CORRECTED: our DPS wire is the COMPACT `<C T=..>` dialect (NOT WebCheck's verbose
`operationtype='-8'` COM layer). `StringXML` maps `abs(-8)=8` → `<C T='8'>`.

```xml
<RQ V='1'><DAT FN='..' TN='..' DI='{lnd}' ZN='0' V='1'>
  <C T='8'>
    <P N='1' ... NM='ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ ДЕРЖАТЕЛЮ ЕЛЕКТРОННОГО ПЛАТІЖНОГО ЗАСОБУ' SM='{sum}' .../>   ← NO TX=
    <M N='2' T='0' NM='{card}' SM='{sum}' PA.. PB.. PC.. PD.. PE.. PF.. PSNM.. RRN../>   ← card leg
    <E .../>
  </C><TS>{ts}</TS>
</DAT><MAC .../></RQ>
```
`<C T='8'>` is THE identity (8 free; ShiftOpen=108). Card `<M>` leg `T='0'` (paymentid≥2 →
card form). NO `<TX>` (not a VAT good). NO cash `<M>` — cash-out is a LEDGER effect only.

---

## V0 — RED-first pins (TEST-ONLY, write + watch fail FIRST)

1. **Wire pin:** build an EPZ canonical → assert the signed body contains `<C T='8'>` and
   the fixed good `NM='ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ…'` (NO `TX=`) and the card `<M …
   T='0'>` leg. (1-byte diff sensitivity on `T='8'`.) NOT verbose `operationtype='-8'`.
2. **Guard-3c pin (INV-21):** EPZ with `sum > cash_on_hand` → 422 `CashInsufficient`
   pre-inbox, row-less (no doc minted). errCode-47 analog. Second pin: in-lease re-check
   (TOCTOU) refuses pre-mint.
3. **Ledger pin:** after an EPZ ACK of sum X, `derive_cash_on_hand` drops by X
   (`− epz_out`). Card payform turnover +X; cash payform unchanged.
4. **Z `<EPZ>` pin:** a shift with N EPZ ACKs of total S → Z payload
   `epz = Some(EpzTotals{ epc: N, epcs: 0, epsm: S })` → emitter `<EPZ EPC='N' EPCS='0'
   EPSM='S'>`. (STOP-S2: this MUST land in the same PR as the policy flip.)
5. **z-quiescence pin:** a non-terminal EPZ doc blocks Z-close (mirror the L3 service-io
   fix `9f9151e`).
6. **paymentid pin:** `paymentid < 2` → 422 (card-form-only).

## V1 — online core (seam map, dossier §6 a–h,j)

enums (a) → CommandType+maps (b) → policy flip `CashAdvanceEpz→Signable` (c) → wire
builder (d) → ledger `−epz_out` + `aggregate_shift_epz` + 3 callers (e) → guard-3c
in-lease (f) → convert guard-3c pre-inbox + EPZ canonical + **populate `ZReportPayload.epz`**
(g) → stage_sign supported `WireArtifactKind` (h) → z-quiescence `+'CASH_ADVANCE_EPZ'` (j).

## V2 — offline (bimodal)

stage_offline_ack EPZ arm (i) + update the N-variant tripwire comment + drain + offline
guard-3c-at-ingress (durable local ledger) + bimodal e2e (offline EPZ → OFFLINE_LOCAL_ACK
→ drain → ACK).

## V3 — fuzzer (MANDATORY — GREEN, not known-red)

- `Op::OnlineEpz(DpsScript)` + `Op::OfflineEpz` in `tests/invariant_fuzzer/op.rs`.
- `apply_epz` in `model.rs`: subtract `epz_out` from model cash; guard-3c refusal
  (`NoMutation`) when `epz_sum > cash_on_hand`; EPZ = card payform for turnover.
- `has_z_quiescence_blocker` (or the model's z-quiescence check) MUST count non-terminal
  EPZ docs.
- **Teeth (prove empirically):** (a) revert guard-3c → seeded harness driving
  EPZ-over-drawer goes RED; (b) revert z-quiescence EPZ inclusion → seeded
  `[OfflineEpz, ZReport]` goes RED. Include both seeded harnesses.
- Update `docs/FUZZER_TIER2_RAGE_DOSSIER.md` uncovered-surface map.

---

## Invariants — HOLD and state each as preserved

- **INV-21** (готівка≥0): guard-3c dual-site, errCode-47, fail-closed. THE core invariant.
- **z-quiescence** (#192/P1): EPZ in the non-terminal blocker set.
- **STOP-S2:** Z `<EPZ>` populated in the SAME PR as the ingress/policy relaxation.
- **INV-6** (full canonical payload): EPZ `<L>` carries all card requisites, not summary.
- **No network/crypto in write txn; short txn:** guard-3c reads ledger in-lease, no network.
- **advance-at-SEND / D2:** EPZ online issuance advances chain at Sending→Sent; no special
  rollback. Pre-SENT reject → Rejected(D2); post-SENT ambiguous → RMR.
- **Idempotency:** EPZ carries idem key like any receipt.

---

## Verification gate (run YOURSELF on the FINAL commit — do not report on a subset)

```
cargo nextest run -p prro --features test-support     # full suite, 0 failed
cargo fmt -p prro -- --check                           # clean
cargo clippy -p prro --all-targets --features test-support -- -D warnings   # 0
```
Plus **teeth-proof**: revert guard-3c → RED; revert z-quiescence EPZ → RED; restore.
Report the ACTUAL final-commit numbers, not an intermediate run.

## Delivery format (required 7)

Intent completed / Files changed / Tests+checks run (with numbers) / Result / Known risks
/ **Invariant check (each of the above, held)** / Suggested next step.

Commit per slice (V0→V1→V2→V3); the architect verifies + 2-lens reviews + merges.
