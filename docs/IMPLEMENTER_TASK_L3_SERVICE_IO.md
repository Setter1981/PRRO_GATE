# Implementer contract — L3: service cash-in/out (внесення/видача) + Z `<IO>` + INV-21 guard-3b

**Reads with:** `docs/FISCAL_PAYFORM_LEDGER_DOSSIER.md` (§1 reference verbatim, §L3). Surface map (file:line) is the ground truth below. **Base:** main `875bd3e` (L0+L1 merged). **Discipline:** strict RED-first TDD. Hot zones = `write_path` + `ingress` + `xml` → preserve frozen invariants.

## 0. Scope
**IN:** wire `DocType::ServiceIn` (службове внесення) + `DocType::ServiceOut` (службова видача) end-to-end; build the Z `<IO>` aggregation **in the same PR** (STOP-S2 mandate); extend `cash_ledger` with service-in/out terms; **INV-21 guard-3b** — a `ServiceOut` over cash-on-hand is refused (pre-inbox + in-lease), mirroring guard-3a.
**OUT:** the zeroing-видача at close (`shiftCashInOut`/`cash_disposition`) = **L4, separate**; **EPZ** (`CashWithdrawal`, op-8) STAYS fail-closed (guard-3c out of scope); no migration.

## 1. Ground-truth seams (from the surface map — file:line, all rel `rust/prro/src/`)
- **Relax gate A:** `runtime/ingress/policy.rs:56-59` — move `ServiceIn | ServiceOut` from `Unsupported` → `Signable`. Leave `CashWithdrawal | PeriodicReport` Unsupported. Update test `cash_movement_and_periodic_are_unsupported:93` (it's the teeth — must fail then be corrected).
- **Relax gate B:** `stage_sign.rs:59-70` add `WireArtifactKind::ServiceIn|ServiceOut`; `:154-169` remove them from `UnsupportedDocType`, add `Ok(...)` arms; the compiler cascades every `match kind`.
- **check_type:** `stage_send.rs:392-403` add `ServiceIn|ServiceOut => DpsCheckType::ServiceChk` (WebCheck `SubmitCheck` code 3, verified).
- **Wire XML (additive, no blocker):** `xml/mod.rs` — add `ServiceCheckPayload{header, local_number, amount_kop, name}`; `CanonicalDoc::ServiceIn/ServiceOut` (enum ~:804); `emit_service_check` → `<C T="2">` + `<I NM SM T='0'/>` (in) / `<O N='1' T='0' SM=.../>` (out) + `<E .../>` (WebCheck shape). Dispatcher arms.
- **Payload:** `stage_sign.rs:1221-1366` — `ServiceIoJson{amount_kop, name}` parse + `build_canonical_doc` arms. Carries `schema_version` (invariant #7).
- **cash_ledger:** `cash_ledger.rs:56-58` — the `TODO(L3)` is the exact point: extend `derive_cash_on_hand(opening, sell, ret, service_in, service_out)` = `... + service_in − service_out`. Extend `aggregate_shift_cash` (:78) + `aggregate_shift_cash_tx` (:125, inline SQL :133 `IN ('SELL','RETURN')` → add `'SERVICE_IN','SERVICE_OUT'`) + routing. `reconcile_opening_anchor:296` auto-includes.
- **★ Z `<IO>` aggregation (STOP-S2) — the emitter is DONE, the pipe is missing:**
  - `xml/mod.rs:1313-1326` already emits `<IO NM SMI SMO T="0">` from `ZReportPayload.service_sums` — **zero XML change.**
  - `db/repositories/fiscal_documents.rs:781` — add `list_shift_issued_service_docs` (`doc_type IN ('SERVICE_IN','SERVICE_OUT')`).
  - `convert.rs:337-346` — add `service_sums: Vec<ZReportServiceIoOut>` (`{name, sum_in_kop, sum_out_kop}`) to `ZReportOut`, `#[serde(skip_serializing_if="Vec::is_empty")]` (mirror `tax_summaries:342`); populate in `aggregate_z_payload_for_shift:900`.
  - **★★ THE ONE MUST-VERIFY SEAM (map's flagged gotcha):** the `ZReportJson` struct in `stage_sign.rs` (~:1244, the `ZReport` parse arm) is the serialization HANDOFF `ZReportOut → ZReportPayload`. It MUST gain `service_sums` (with `#[serde(default)]`) and pass it into `ZReportPayload.service_sums`, else `<IO>` is EMPTY even though convert serialized it. **Write a pin that a live Z after a видача carries a non-empty `<IO>` — this is what proves the handoff.**
- **STOP-S2 pin:** `z_builder.rs:35-55` — `FULL_Z_SURFACE_READY:55` STAYS `true` (the IO half IS built this PR). The coupling-pin tripwire test must confirm ingress-relaxed ⟺ IO-built consistency. Do NOT relax `CashWithdrawal`/`acquirer_slip` (EPZ stays closed).
- **INV-21 guard-3b:** pre-inbox `convert.rs:843-868` — new `ServiceOut` arm mirroring guard-3a (`amount_kop > cash_on_hand → CashInsufficient` 422, row-less). In-lease `stage_acquire.rs:759` — extend the condition to also arm `DocType::ServiceOut` (same `aggregate_shift_cash_tx`+`derive_cash_on_hand`, same `CashInsufficientInLease`).

## 2. Migrations: NONE. `fiscal_documents.doc_type` + `ingress_inbox.operation_type` CHECKs already allow SERVICE_IN/OUT (`sql/001:211,100`). `shifts.cash_balance_kop` exists (`sql/009`).

## 3. RED pins (write first, watch fail)
1. `pin_service_in_issues_and_counts` — a ServiceIn(100.00) issues (ACK) → cash-on-hand += 10000.
2. `pin_service_out_issues_and_counts` — with drawer 100.00, ServiceOut(30.00) issues → cash-on-hand = 7000.
3. `pin_service_out_over_drawer_refused_pre_inbox` — drawer 0, ServiceOut(1.00) → row-less 422 CashInsufficient (guard-3b pre-inbox).
4. `pin_service_out_in_lease_toctou` — in-lease guard refuses a ServiceOut that passed pre-inbox but exceeds same-tx cash (mirror the L1 in-lease pin).
5. **★ `pin_z_carries_io_section`** — open shift, ServiceIn 50 + ServiceOut 20, close → the signed Z XML contains `<IO ... SMI="5000" SMO="2000"/>` (proves the ZReportJson handoff — the map's flagged seam).
6. `pin_service_out_teeth` — revert guard-3b → pin 3 RED (teeth).
7. `pin_epz_still_closed` — CashWithdrawal still 422 Unsupported (STOP-S2 EPZ stays shut).
8. `pin_stop_s2_coupling` — the tripwire: ingress-relaxed-for-service-io ⟺ Z-IO-built.

## 4. Fuzzer-impact (MANDATORY — `feedback_fuzzer_tracks_features`)
Add `Op::ServiceIn`/`Op::ServiceOut` to the alphabet; extend the RefModel cash accumulator (service-in `+`, service-out `−`, with the guard-3b INV-21 refusal predicted); add a **guard-3b teeth** in the SEEDED harness (not the random `op_sequences` proptest — that's cash-oracle-free by the #257 CI-fix): revert guard-3b → `harness_online_seeded` RED. Fence any new cross-mode service-io divergence as known-red (W-ledger-fidelity follow-up). Do NOT re-arm the cash-oracle in `drive_sequence`.

## 5. Process (CI-lesson from #257 — non-negotiable)
- **Gate = the EXACT CI command:** `cargo nextest run -p prro --features test-support` (FULL 1824+ suite, NOT a 2-binary subset) + `cargo fmt -p prro -p prro_crypto -p prro_escpos -- --check` + `cargo clippy -p prro --features test-support --all-targets -- -D warnings`. Paste all three green.
- Teeth-proof: revert guard-3b → run → RED → restore → green. Leave `git status` CLEAN (no un-restored reverts).
- Strict RED-first; commit each RED→GREEN; minimal diff; TEST-ONLY not in prod.

## 6. Invariant check (state in PR): #1 (guard = pure SELECT), #2 (in-lease under lease), #4 (row-less refuse idempotent), #7 (ServiceIoJson carries schema_version), #8. STOP-S2 upheld (IO built same PR; EPZ stays closed). Deliver 7-point format; PR, do NOT merge (I review 2-lens + verify gate+teeth + merge).
