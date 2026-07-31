# Mutation-Testing Teeth Backlog (FW-1, round 1)

**Source:** cargo-mutants whole-workspace run on `origin/main` @ `3e2088b` (CCX63 box), analyzed at ~30% progress (survivors so far).
**Analysis:** per-survivor real-vs-equivalent workflow (12 fiscal-relevant survivors), each confirmed against exact `origin/main` code + existing test coverage.
**Verdict tally:** 4 REAL · 6 LOW_VALUE · 2 EQUIVALENT.

The production **code is correct**; a "survivor" = a mutation NO test catches = a **teeth-gap in the oracle**, not a bug. Fix = add a RED-first oracle that dies under the mutation. Prove teeth by applying the mutation → test goes RED → revert → GREEN (teeth-canary discipline, per PR #257).

---

## A. REAL teeth-gaps — write RED-first oracles

### A1. ✅ DONE (PR #275) — 🔴 HIGH — `cash_ledger_epz_leq` (INV-21 cash over-draw / double-spend)
- **Where:** `services/cash_ledger.rs:343` — EPZ loop of `aggregate_shift_cash_tx` (the **tx/in-lease** variant; distinct from pool `aggregate_shift_epz`). Mutation `if sum_kop <= 0` → `> 0`.
- **Why real:** flips "sum positive EPZ" → "skip positive EPZ" ⇒ `epz_out = 0` whenever a **prior** EPZ exists ⇒ cash-on-hand computed **too high** by Σ(prior EPZ) ⇒ the INV-21 in-lease cash-floor re-check (`stage_acquire.rs:806-838`, the TOCTOU guard PR #257 hardened) **accepts an over-draw it must refuse**.
- **Fiscal impact:** cashier who already paid out cash via EPZ can issue a further RETURN/SERVICE_OUT/EPZ that drives the physical drawer **below zero** — unbacked cash out, Z-close shortfall, negative drawer vs reality. The exact cash-double-spend the guard exists to stop.
- **Why every test missed it:** epz.rs Pin-3 hits the **pool** variant (`aggregate_shift_epz`), not the tx copy; S11/S12 drive only a **first** EPZ (no prior EPZ to mis-drop); invariant_fuzzer's `ExpectedNoMutation` teeth are correctly shaped but never sample the deep conjunction (uniform `CASH_AMOUNT_KOP=15000`, flat 1..=8 op sequences).
- **⚠️ CANARY CORRECTION:** the originally-drafted **serial** end-to-end test (`matrix_s12b`, two-EPZ over-draw through `handle_command`) has **NO teeth** — proven by the mutation canary (passed under the mutation). Root cause: the in-lease guard only runs under `Channel::Online` as a **TOCTOU re-check**, and a **serial** over-draw is refused earlier by pre-inbox **guard-3c** (which reads the *pool* aggregate `aggregate_shift_epz`, not line 343), masking the in-lease read. The serial path can't distinguish the mutant.
- **RED test (shipped):** `pin_epz_tx_aggregate_counts_epz_out` in `tests/epz.rs` — a **direct** twin of Pin 3 for the tx aggregate via `with_immediate`: seed a committed EPZ ACK → assert `aggregate_shift_cash_tx`'s `epz_out == 3000`. Canary: mutate `<= 0`→`> 0` @ line 343 → **FAIL** (epz_out 0 vs 3000); revert → **PASS**; all other EPZ tests stay green.
- **Durable second layer (optional):** bias fuzzer strategy (amount diversity + a two-EPZ drain→overdraw sequence) so the concurrency/TOCTOU end-to-end path also gains coverage — gives the existing `ExpectedNoMutation` teeth a reachable trigger.
- **Lesson:** verify teeth EMPIRICALLY (mutate → RED). A survivor's plausible reachability story (here: "serial two-EPZ triggers the in-lease guard") can be wrong; only the canary proves it.

### A2. ✅ DONE (PR #276) — 🟠 MEDIUM — `validate_evidence_ok` (force-seam evidence validation disabled)
- **Where:** `db/repositories/shifts.rs:475` — `validate_evidence_json` body → `Ok(())`. First guard of BOTH `force_to_error_with_audit` / `force_to_manual_reconciliation_with_audit`.
- **Why real:** disables size cap (8 KiB) + JSON-shape check ⇒ malformed/oversized `evidence_json` passes ⇒ stamped verbatim into the **forensic Critical audit** row of the operator "force" seam (ЧП-из-ЧП manual-recon surface). Poisons audit JSON integrity + bypasses the caller-bug refusal.
- **Coverage miss:** force-seam source-guard tests always pass the VALID `EVIDENCE` const; the invalid-JSON test hits `senior_cashier_close`'s **separate inline** copy, not this fn.
- **RED test:** extend `tests/shifts_force_seam_source_guard.rs` — feed non-JSON + valid-but->8KiB evidence to both seams on an allowed source (Opened) ⇒ assert `Err(InvalidEvidenceJson)` + state unchanged + zero audit rows.

### A3. ✅ DONE (PR #276) — 🟠 MEDIUM — `force_rmr_negation` (lost operator audit attribution)
- **Where:** `db/repositories/shifts.rs:674` — `actor_id.filter(|s| !s.is_empty())`, delete `!`. In `force_to_manual_reconciliation_with_audit`.
- **Why real:** flips "keep non-empty actor" → "keep only empty" ⇒ real operator id (`Some("op-007")`) → `None` ⇒ every force-to-manual audit row records `actor = NULL`. State/CAS/outcome/audit-count unchanged (that's why it survives), only the `actor` column corrupts.
- **Fiscal impact:** destroys non-repudiable "who forced the shift into manual reconciliation" trail on a legally-sensitive action (auditability = PRRO priority #2).
- **Coverage miss:** the 9-case seam test asserts variant/state/**count**, never reads `audit_log.actor`.
- **RED test:** `force_to_manual_records_operator_actor_in_audit` in `tests/shifts_force_seam_source_guard.rs` — `SELECT actor FROM audit_log WHERE event_type='SHIFT_FORCE_TO_MANUAL_RECONCILIATION'` == `Some("op-007")`.

### A4. ✅ DONE (PR #276) — 🟠 MEDIUM — `cp1251_table` (false receipt-refusal on common UA typography)
- **Where:** `xml/cp1251.rs:48+` — 17 deletable `encode_char` match arms. Hit at `build_canonical_xml` (mod.rs:911) over operator free-text (item names, `<L>` header/footer, bank/device names).
- **Why real:** deleting an arm ⇒ `encode` returns `Err(Cp1251Unmappable)` ⇒ **whole receipt build FAILS**. Several deleted codepoints are common in Ukrainian fiscal text: `«` U+00AB→0xAB (opening guillemet in essentially every legal company name — note closing `»` is a *kept* arm, so breakage is asymmetric), `°` U+00B0 (spirits "40°"), `·` U+00B7, plus `… – ' ' • „`.
- **Fiscal impact:** a legitimate receipt with `«` or `°` is **refused instead of fiscalized** — false receipt-refusal, sale can't be issued (availability corruption; fail-closed, not silent-byte).
- **Coverage miss:** unit tests pin only *kept* arms (201C/201D/2014); goldens use ASCII+core-Cyrillic+і/ї.
- **RED test:** `deleted_table_arms_round_trip_to_exact_bytes` in cp1251.rs `#[cfg(test)]` — pin all 17 (char→byte); **one test kills all 17 arms**. Companion boundary test `guillemet_company_name_round_trips_through_cp1251` in xml/mod.rs (item «Вино 12°» fiscalizes end-to-end).

---

## B. ✅ DONE (PR #277) — DEAD CODE — retired via mutants.toml exclude_re (NOT source-deletion)

Mutation testing surfaced three orphaned repo helpers whose survivors reflect **unreachable code**, each with a **stale doc-comment** that risks a future implementer re-wiring dead logic:

| fn | file | orphaned by | doc-rot hazard |
|---|---|---|---|
| `max_submitted_lnd` | `fiscal_documents.rs:1000` | M2-N4 ruling (→ `submitted_above_lnd` sfn-membership) | doc back-links |
| `last_ack_unsigned_xml_sha256` | `fiscal_documents.rs:1097` | AUD-L6-1 (boot uses `last_issued_...`) | "do-not-widen" pin, twinned w/ last_server_fiscal_no |
| `current_open_or_draining_session_id_tx` | `offline_sessions.rs:478` | B10 removed `resolve_offline_dps_code_forced` | doc references a **deleted** symbol |

**Decision (revised after per-fn verification):** retired via cargo-mutants `exclude_re`, NOT deleted — `last_ack_unsigned_xml_sha256` is a deliberate A24 do-not-widen pin, `current_open_or_draining_session_id_tx` is offline-drain hot-zone (delete-vs-rewire = architect call), only `max_submitted_lnd` is a clean orphan. Deleting fiscal hot-zone source on incomplete context outweighs the benefit. Verified via `--list` (11 mutants present without / absent with the block). Physical deletion of the 2 true orphans remains a deferred architect pass.

---

## C. Skip / low / equivalent (rationale one-liners)

- `zreport_neg_lt` (LOW) — `< 0`→`<= 0` differs only on `sum_kop==0`, already rejected upstream by L5 G3 `ZeroPaymentAmount` (422 pre-inbox). Optional 1-liner if touching the convert.rs test cluster.
- `cas_isolation_audit_none` (LOW) — defensive branch unreachable under BEGIN IMMEDIATE single-writer; would need a test-visibility seam. Skip unless locking model weakens.
- `hex_digit_sub` (LOW) — latent: `unseal_jks` has zero production callers (at-rest sealing deferred). KAT test optional; revisit when sealing is wired.
- `hex_to_ski_or` (**EQUIVALENT**) — `(hi<<4) | lo` ≡ `^` because `hex_digit` guarantees both nibbles ∈ 0..=15 (disjoint bits). Proven for all 256 pairs. No action.
- `drain_outcome_str` (LOW) — `"xyzzy"` changes only diagnostic error-message text on a "should-never-happen" structural-drift halt; no fiscal effect. Optional variant-map test.

---

## D. Meta findings
- **Fuzzer blind spots exposed** (feed back into invariant_fuzzer): (1) uniform `CASH_AMOUNT_KOP=15000` — no amount diversity to trigger boundary/aggregation flips; (2) shallow flat 1..=8 op sequences rarely reach deep conjunctions (e.g. two-EPZ drain-then-overdraw). Both surfaced by A1.
- **ROI confirmed:** one genuine fiscal-correctness hole (A1 cash over-draw) that unit + fuzzer + scenario-matrix ALL missed, plus 2 audit-integrity gaps + 1 encoding fragility + 3 dead-code cleanups. Justifies the run.
- **Pending:** run is ~30% — re-run this analysis on the FULL survivor set at completion; write_path/reconciliation core largely not yet reached.
