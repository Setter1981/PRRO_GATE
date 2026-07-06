# PR-R (RETURN+) — Phase-0 Dossier + R-0a determination

Base: `feat/aprime-r-return` @ `8f134e1` (post-Z1 main). Read-only recon.

**Verdict: R-0a core wire form is CORRECT (GO for RED-first pins). One fail-open
finding needs an architect ruling before code — the `return_check_number`
silent-drop.**

---

## R-0a — RETURN wire-correctness (the STOP-R1 gate)

| aspect | verdict | evidence |
|---|---|---|
| check-type `<C T="1">` | **CORRECT** | Python `dps_xml.py` (T=1=повернення) + WebCheck `docs/webcheck_reverse/WEBCHECK_ANALYSIS.md:98` (T=0 SELL / T=1 RETURN) + golden `tests/goldens/xml/return.bin`; `xml/mod.rs:805` `emit_check(p,"1",..)` |
| item/payment shape | **CORRECT** | identical to SELL — direction carried by `WireArtifactKind`, not the payload (`stage_sign.rs:1067`); `check_payload_from` shared |
| payment sign / Z-route | **CORRECT** | RETURN → `sum_out_kop`/SMO (`convert.rs:419`), + the TXS SMO/TXO side from Z1; positive on the wire |
| golden coverage | present | `goldens_byte_equiv.rs::return_doc()` + `regenerate.py:135-162` (Python oracle emits `<C T="1">`) |

→ The compact `<C T="1">` RETURN form is **byte-faithful to the Python-parity
ground truth** and adequately pinnable. R-0a core = **GO**.

### R-0a FINDING (architect ruling needed) — `return_check_number` fail-open

`return_check_number: Option<String>` is defined on the ingress DTO
(`dto.rs:58`) and captured by adapters, but is **NEVER consumed** downstream —
zero uses in `stage_sign` / `stage_send` / `xml`. It is **accepted at ingress
then silently dropped** (no error, no audit).

- DPS does **not require** it in the compact form: neither Python `_build_check`
  nor WebCheck emit ORDERRETNUM; the verbose `check01.xsd:376` has ORDERRETNUM
  as *optional*, but the gateway ships the compact `<C T=>` form (same as the Z
  report used the compact WebCheck form, not zrep01.xsd). So the emitted wire
  bytes are correct **without** it.
- BUT: silently accepting-and-dropping a fiscal field is a **fail-OPEN**, which
  is **inconsistent** with the project's fail-closed posture — `raw_frames`
  (`convert.rs::RawFramesNotSupported`) and `acquirer_slip`
  (`AcquirerSlipMappingDeferred`) both **fail-CLOSED** with typed errors when a
  captured field can't be honored.

**Ruling options:**
1. **Fail-closed reject (recommended)** — reject a non-null `return_check_number`
   at ingress/convert with a typed `422 UNSUPPORTED`-class error, mirroring
   `raw_frames`/`acquirer_slip`. Consistent, minimal, honest. A client that
   needs the original-receipt link learns it isn't supported instead of
   sending a defective return silently.
2. **Emit it** — plumb `return_check_number` → the wire. Requires DPS
   confirmation the compact form accepts ORDERRETNUM (Python/WebCheck say it is
   NOT emitted → likely rejected/ignored by DPS; would need live confirmation).
3. **Accept-and-drop, documented** — keep the current behavior but add an
   explicit comment + audit note that it is an intentional deferral.

This is the R-0a ruling the contract reserved ("если некорректно — фикс по
рулингу STOP-R1"). Wire bytes are correct; the fail-open is an architect call.

### R-0a RULING (LOCKED 2026-07-06) — option 1: FAIL-CLOSED REJECT

Architect ruled **option 1 (fail-closed reject)**. Rationale: the fail-closed
family (`raw_frames`/`acquirer_slip`) exists so the gateway does NOT accept
semantics it cannot deliver. A client sending `return_check_number` believes the
fiscal return is linked to the original receipt — but no link exists or can
(the compact dialect does not carry it: Python-prod 4yr + WebCheck never emit
ORDERRETNUM). A silent drop (option 3) leaves the client in that false belief
until a tax audit; an in-code doc-comment is invisible to the client. A `422`
makes the client learn the truth on the first request. Option 2 (emit) rejected
— emitting an attribute the reference serializers never emit = an unverified
wire surface without live confirmation; not now. Timing: cheap to close the door
pre-pilot, expensive after; reversibility is clean (if DPS/law later requires
the link — verbose format / cashback class — the typed 422 becomes a
live-verified emit path, and no deployed client depends on drop semantics).

**Implementation requirements (in PR-R scope):**
1. Reject at `convert`, mirroring the family: typed **422**, strictly
   **pre-mint** (invalid-ingress class → `audit_log` only, ZERO
   `fiscal_documents` row, `lnd` untouched). Do NOT break DTO parsing of the
   field — accept it on deserialization, refuse it typed.
2. Doc-comment MUST name the ground-truth WHY (compact dialect does not carry
   it; emit = a future live-verified enhancement, NOT data loss).
3. RED-first pins: non-null `return_check_number` → 422 + zero mint; null/absent
   → RETURN flows as today.

---

## Fuzzer Phase-2 surface (buildable)

**Key fact: a RETURN is chain-wise IDENTICAL to a SELL** — consumes an `lnd`,
advances the FN chain seed at the same boundary (online: SEND/ACK; offline:
`OFFLINE_LOCAL_ACK`). Only the wire type ("1") and the Z direction (sum_out/SMO)
differ. So the RefModel `apply_return` mirrors `apply_sell`.

- **Op alphabet** (`tests/invariant_fuzzer/op.rs:83`): add `OnlineReturn(DpsScript)`
  + `OfflineReturn` (mirroring `OnlineSell`/`OfflineSell`).
- **RefModel** (`tests/invariant_fuzzer/model.rs:198` dispatch, `:286`
  `apply_sell`): add `apply_return` — same lnd/seed/codes bookkeeping as
  `apply_sell`; the abstract model tracks the chain (lnd/seed/state) — the
  sum_out distinction is a differential concern, not a model-state field.
- **Generator** (`tests/invariant_fuzzer/strategy.rs:47` prop_oneof): add
  `dps_script().prop_map(Op::OnlineReturn)` + `Just(Op::OfflineReturn)` with
  weight so Return sequences actually appear in runs.
- **Differential + teeth**: harness runs Return through prod↔model; teeth =
  revert a model arm → RED.

## e2e SELL → RETURN → Z (buildable, aggregation-level)

RETURN lands in the Z sum_out side (payments) + SMO/TXO (TXS, from Z1). Verified
in Z1 that `aggregate_zreport`/`derive_z_report_tax_summaries` route
`DocType::Return` → SMO. The e2e pins a shift with a SELL + a RETURN → Z, asserts
the RETURN in SMO/TXO and sum_out.

## Invariants (to preserve in PR-R)
- **#5 / INV-08** — offline RETURN respects code/time limits; the existing
  offline-issuance arm (code consumption) must NOT be weakened.
- **#6** full payloads · **#8** Z-quiescence closes only via the C10 gate
  (RETURN opens no bypass) · **D6** no config knob · **zero migrations** ·
  A.3 core / reaper / binding / shift guard-matrices untouched.

## Recommendation
R-0a core is GO. **Halting for the architect's `return_check_number` fail-open
ruling** (recommend option 1, fail-closed reject) before writing PR-R — it
determines whether PR-R adds the fail-closed ingress validation. Everything else
(RED-first return-wire pins, fuzzer Phase-2, e2e) is scoped + buildable.
