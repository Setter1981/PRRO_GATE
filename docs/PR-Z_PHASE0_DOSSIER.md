# PR-Z (A2-Z / W4-Z2) — Phase-0 Dossier + STOP-Z determination

Base: `feat/aprime2-z-surface` @ `80f8ced` (post-reaper main). Read-only recon (7-lens
ultracode fan-out + targeted follow-ups). **Verdict: STOP before code — two structural
blockers turn the contract's "surface + flip + live-e2e" into an unbuilt write-path
increment. Architect adjudication required.**

---

## 1. The seam (verified anchors)

| element | anchor | state |
|---|---|---|
| release gate | `z_builder.rs:48` `FULL_Z_SURFACE_READY = false` | closed |
| gate fn | `z_builder.rs:68` `ensure_full_z_surface_ready() -> Result<(),ZSurfaceNotReady>` | fail-closed |
| **only consumer** | `inline.rs:519` (SELL/RETURN-only orchestrator) | fail-closes Z |
| tripwire (unit) | `tests/z_builder.rs:89` `assert_eq!(…, Err(ZSurfaceNotReady))` | green |
| tripwire (runtime) | `inline.rs:520` `debug_assert!(surface.is_err())` | **panics on flip** |
| aggregation | `convert.rs:316` `aggregate_zreport` (payments-only) | summary-only |
| hardcode site | `stage_sign.rs:1080/1091/1096` `tax_summaries/service_sums=Vec::new()`, `epz=None` | empty |
| XML emitter | `src/xml/mod.rs:660-762` + `tests/xml_w4_z1_z_report_aggregations.rs` | **built + green** |

---

## 2. The three Z sub-surfaces — buildability

### TXS (tax-group turnover) — REAL WORK, no migration
- Data is fully persisted (baseline migration 001): each issued receipt's stored `CheckOut`
  carries `items[]` with `tax_group_1/2 + sum_kop`; each doc has `signing_config_snapshot_id`
  FK → `signing_config_snapshots` (canonical rates + driver→canonical mapping), stamped
  atomically by `stage_acquire`.
- Build a NEW `derive_z_report_tax_summaries` (mirrors `tax_summary.rs:409
  derive_check_tax_summaries`; Python `dps_xml.py:444-458` short/full-form fallback;
  TXI/TXO via `calc_tax`). Group by `tax_id`, SELL→SMI / RETURN→SMO.
- Ground truth: `goldens_byte_equiv.rs:z_report_extended_doc()` (tx=1: smi 12000/txi 2000;
  tx=2: smi 700/txi 45) + Python `shift_aggregation.py:61-74`.
- **Zero migration.** The one wrinkle: `list_shift_issued_receipts` (`fiscal_documents.rs:551`)
  returns `(doc_type, payload_json)` without the snapshot FK → the aggregator must also load
  the per-doc tax snapshot (repo fn exists). Back-compat: pre-W4-Z2a NULL-snapshot docs need a
  short-form fallback (Python already does this).

### IO (service in/out) — LEGITIMATELY ALWAYS-EMPTY (not work, not blocked)
- `SERVICE_IN`/`SERVICE_OUT` exist as `DocType`/`CommandType` BUT are classified
  `CommandClass::Unsupported` at the ingress boundary (`policy.rs:56`) → **HTTP 422 before any
  inbox write**. No service doc can ever be minted; `list_shift_issued_receipts` is SELL/RETURN-only.
- `service_sums = Vec::new()` is the correct, audited state. Pin it as intentionally-empty; no
  aggregation code, no migration.

### EPZ (card-acquiring) — ALWAYS-EMPTY (spec-deferred upstream)
- EPZ (`EPC/EPCS/EPSM` = card-op count / commission / amount) needs acquirer-slip data, which is
  **fail-closed at convert** (`convert.rs:514` `AcquirerSlipMappingDeferred` — an OPEN spec
  question, W4-Z1 §Q1). No card-op source data can enter the ledger.
- `epz = None` is correct. Completing EPZ requires (a) resolving the deferred acquirer-slip→EPZ
  spec mapping AND (b) a migration to persist commission/terminal — **out of PR-Z scope.**

**Net surface work = TXS only.** IO + EPZ are correctly-empty-by-construction (no ingress).

---

## 3. THE BLOCKER — no live-Z dispatch path exists (STOP)

The production binding `InlineWritePath` (`inline_binding.rs:53`, wired at the DI root by flip
#230) delegates **100%** to `inline::run` with NO operation_type routing. And `inline::run`
**fail-closes at the top, before build/acquire**:
- `Z_REPORT` / `SHIFT_CLOSE` → `inline.rs:512-539`: terminalise inbox → `ZSurfaceNotReady` (501),
  guarded by `debug_assert!(surface.is_err())` (`inline.rs:520`) whose comment states *"a later
  A2.4-Z piece owns the live-Z path."*
- `SHIFT_OPEN` → `inline.rs:541-556`: terminalise → `SHIFT_OPEN_NOT_SUPPORTED`.

The write-path STAGES (`stage_acquire`/`stage_send`/`signer_guard`) DO handle Z/shift doc types —
but the production orchestrator does not route them. So:

> **Contract step 3 (flip `FULL_Z_SURFACE_READY`) is incoherent alone**: the flag's only consumer
> is `inline.rs:519`; flipping it to `true` trips `debug_assert!(surface.is_err())` (test panic),
> and in release builds `inline::run` would still terminalise Z as "not ready" — now lying. The
> flip is meaningful ONLY together with a real live-Z dispatch arm.

> **Contract step 4 (e2e `boot → SHIFT_OPEN → SELLs → Z-close` via the live binding) is unbuildable**:
> neither `SHIFT_OPEN` nor `Z_REPORT/SHIFT_CLOSE` dispatches through the live binding. Building it
> = wiring Z **and** shift dispatch into `inline::run` (Z acquire/chain-seed, D5 sign already in
> `build_z_canonical`, send/confirm/finalize, quiescence orchestration, `SHIFT_CLOSE` state edges
> 4/12 + ambiguous-timeout→RMR per M3b §16.7) — a full write-path increment with its own design +
> review surface, in hot zones.

This is the STOP: the contract conflates **"complete the aggregation surface"** (small — TXS only)
with **"flip the gate + live e2e"** (a separate, unbuilt live-Z + live-shift write-path piece).

---

## 4. Also confirmed (green, orthogonal)
- **C10 quiescence pins** (`tests/z_quiescence.rs`, 9 cases): exercise the receipt state machine
  (what blocks close), NOT the aggregate payload shape. `aggregate_zreport` parses
  `StoredCheckPayments` without `deny_unknown_fields` → adding fields to the aggregated body cannot
  regress them. Orthogonal to the TXS work. ✔
- **e2e harness patterns** exist: `tests/a3_final_binding_flip.rs` (in-memory mock DPS, live
  binding) for integration; `tests/live_dps_extended_smoke.rs` Smoke-7 Z (feature-gated, live DPS)
  for ops — but BOTH still hit the fail-closed Z arm today.

---

## 5. Options (recommend C)

- **A — surface-only (no flip, no live e2e):** Build TXS aggregation + un-hardcode `stage_sign`
  TXS; pin IO/EPZ as intentionally-empty; verify via aggregation-level test
  (`aggregate_z_payload_for_shift → build_z_canonical → stage_sign` → assert TXS in the Z XML).
  Do NOT flip the gate (no live consumer). Closes the AUDIT5-CRIT-2 summary-only debt. Minimal
  diff, honest, no scope explosion. Does not satisfy contract steps 3-4.
- **B — full live-Z:** A + build the A2.4-Z live-dispatch arm in `inline::run` (Z + shift routing,
  acquire/send/finalize, SHIFT_CLOSE edges, quiescence-in-orchestrator) + flip gate + rewrite
  `debug_assert` + live e2e. Large; needs its own Phase-0/design (hot zones). Really "PR-Z live
  dispatch," not "surface."
- **C — split (recommended):** PR-Z now = Option A (TXS surface, gate stays closed). PR-Z2 next =
  the live-Z dispatch increment (Option B delta) under its own contract, where SHIFT_CLOSE edges +
  Z chain-seed + quiescence orchestration get proper design + adversarial review.

**PR-R (RETURN+)** is unaffected by this STOP: RETURN already routes to `sum_out` in
`aggregate_zreport` (verified `convert.rs:328`); the fuzzer `Op` alphabet has no
`OnlineReturn/OfflineReturn` yet (`tests/invariant_fuzzer/op.rs`). It can proceed after whichever
Z path lands.

---

## 6. ARCHITECT RULING (2026-07-06) — PR-Z1 = TXS-only, amended

Ruling: **Option 1 (TXS surface only)**. Option 2 rejected (mixes cool-zone data-shaping with an
A2.2-class hot-zone dispatch increment — risk-profile + review-load blowup, would hold the TXS
debt hostage to a dispatch design defect). Option 3 rejected (the TXS surface is strategy-invariant
— any future live-Z consumes it; this is the "wire the seam" decision-rule; holistic design is
needed for *dispatch*, not aggregation).

**Re-sequence:** PR-Z1 (TXS) → PR-R (RETURN+) → PR-Z2 (live-Z dispatch). PR-R does NOT wait for
Z2 — its `SELL → RETURN → Z` e2e is verified at the aggregation level (sum_out + the TXS side from
Z1). PR-Z2 is a separate contract with a design-dossier BEFORE code (hot-zone: Z+shift routing in
`inline::run`, SHIFT_CLOSE edges, quiescence orchestration, flag flip, `debug_assert` rewrite,
pilot-gate e2e) submitted to the architect for adjudication. The PILOT-GATE e2e (boot→OPEN→SELL→
Z-close via live binding) moves OUT of PR-Z1 into PR-Z2.

**Amendments to Option 1:**
1. IO/EPZ intentionally-empty pins MUST be ground-truth-justified (which wire form: section
   present-but-zero vs absent — different on the wire). If ground truth does not settle it →
   STOP-Z1, do not invent.
2. Flag NOT flipped (contract-consistent: its own doc-comment says "flip IN THE SAME CHANGE that
   completes the surface"; TXS-only does NOT complete it). Tripwire (`tests/z_builder.rs:89`)
   untouched. EPZ fail-closed (`convert.rs:514`, §Q1) NOT weakened.
3. Verification at the aggregation seam: `aggregate → build_z_canonical → sign → XML` vs TXS
   ground-truth pins (fixture ledger → exact expected sections). RED-first.
4. Locked-contract remainder unchanged (adversarial lenses ≥4, nextest gate, invariants, boundaries,
   merge is the architect's).

### Amendment-1 resolution — NOT STOP-Z1 (empty IO/EPZ → ABSENT)

The gateway emits a Z-report as a WebCheck/DPS-reality **compact** doc (`Check.Type::ZREPORT=2`,
`fiscal_server.proto`; tags `<TXS>/<M>/<IO>/<NC>/<EPZ>` — see `xml/mod.rs:18-48`). **No repo XSD
governs the compact format**: `schemas/xsd/check01.xsd` (CHECK/CHECKHEAD/CHECKTOTAL/CHECKPAY/
CHECKTAX/CHECKBODY) and `zrep01.xsd` (ZREP/ZREPREALIZ/…/ZREPBODY) are DIFFERENT (verbose) formats.
Ground-truth authority for the compact form is the production Python serializer + reverse-eng docs
+ the green emitter contract. All agree, and the official XSD structurally corroborates:

| section | empty form | ground truth |
|---|---|---|
| IO | **ABSENT** (no `<IO>`) | Python `dps_xml.py:470-477` (iterates non-empty `service_sums`); Rust `empty_optional_sections_emit_no_txs_io_epz`; corroborated `zrep01.xsd ZREPBODY/SERVICEINPUT/SERVICEOUTPUT minOccurs=0` |
| EPZ | **ABSENT** (no `<EPZ>`) | Python `dps_xml.py:487-491` (`if epz_sums`); Rust emitter test; corroborated `zrep01.xsd ZREPCASH minOccurs=0` |

→ Pins assert the section is ABSENT when empty (present-but-zero would be WRONG per the production
serializer). NOT STOP-Z1: the answer is unambiguous + triangulated.

**DECISIVE grounding (added after WebCheck + docs cross-check):** the project's OWN authoritative
wire-shape spec `docs/superpowers/specs/2026-05-26-w4-z1-dps-xml-wire-shape.md:329` states verbatim:
*"empty `<TXS>` / `<M>` / `<IO>` / `<EPZ>` is legal-valid Z-report (DPS accepts; same as Python with
empty `z_report_data`)"*. §Q5 (:474) confirms PR-Z1 scope ("Live stage_sign Z-report pathway
DEFERRED to W4-Z2", ordering TXS→M→IO→NC→EPZ); :270 IO source (`service_sum`) not plumbed (M5);
§Q1 EPZ deferred → both legitimately empty. This is the strongest grounding — the project's
documented decision, not an inference.

**WebCheck cross-check (decompiled DPS client, `StringXML.cs`):** confirms the TXS CORE — TXI/TXO =
`rate*amount/(100+rate)` (`All.cs:1093 TaxAmountBig`) ≡ `calc_tax` TXAL=0; TXS conditionally emitted
(unknown group omitted); numeric TX mapping (`Directorys.ABCtoNUM`). BUT WebCheck DIFFERS on three
points where the project deliberately chose Python-parity instead: (1) empty IO/EPZ = present-but-zero
(WebCheck) vs ABSENT (project, spec:329); (2) NC/IO order (WebCheck NC→IO) vs IO→NC (project,
spec §Q5); (3) richer TXS attrs (WebCheck +`N`/`DTI`/`DTO`) vs 10-attr Python form. All three are
different-but-DPS-accepted vendor-client variations; the project committed to Python-parity
(spec-documented, 4-yr-prod-validated, test-pinned). PR-Z1 follows the project form (ABSENT). Any
move to WebCheck-parity is a SEPARATE emitter change (breaks `empty_optional_sections`, out of TXS
scope) — tracked follow-up, architect's call.

### PR-Z1 build plan
- **RED-first pins** (aggregation seam): fixture ledger of issued SELL/RETURN → `aggregate_z_
  payload_for_shift → build_z_canonical → stage_sign` → assert Z XML `<TXS>` sections vs ground
  truth (`goldens_byte_equiv.rs z_report_extended_doc` tx=1/tx=2; full-form alphabetical +
  short-form fallback; TXI/TXO via `calc_tax`; SELL→SMI / RETURN→SMO; string-sorted TX). Plus IO/EPZ
  intentionally-empty pins (ABSENT).
- **Implement**: new `derive_z_report_tax_summaries` (mirror `tax_summary.rs:409
  derive_check_tax_summaries`; Python `:444-458` short/full-form fallback) that resolves each
  receipt's raw tax groups via its per-doc `signing_config_snapshots` (FK
  `signing_config_snapshot_id`), aggregates by canonical TX (SELL→SMI/RETURN→SMO), computes
  TXI/TXO. Extend `ZReportJson` (+ optional `tax_summaries`, back-compat) and un-hardcode
  `stage_sign.rs:1080 tax_summaries`. IO=`Vec::new()` / EPZ=`None` stay (pinned intentional).
- **Zero migration.** Back-compat: pre-W4-Z2a NULL-snapshot receipts → short-form fallback.
