# CS-3 Oracle — External Audit Findings & Follow-up Backlog

**VERDICT: `SYSTEMIC`.** The oracle (spec-family rev9, merged as `9e6cf96` / PR #330) is **not safe to
build Bridge → D → E from as-is.** Two independent scenarios violate the core safety property (P2 no
repeated wire / P4 no lost-or-double doc across crash), plus several normative contradictions. **Do not
un-merge** — corrections land as a follow-up PR. **This file is the backlog for that PR** (untracked until
committed with it).

**Root cause (architectural).** The design conflates two distinct guards:
- a **per-FN chain-fork fence** (prevents a *new document* on a stale seed while an ambiguous doc is
  unresolved), which correctly **releases on clean-accept**, and
- a **per-document lifetime call-once marker** (prevents the *same document* from ever wiring twice),
  which **does not exist**.
Because clean-accept releases the FN-fence and there is no per-document call-once, an accepted doc whose
quittance is later lost (`Sent → ErrorRetryable → Sending`) is re-sent → **double issue**. This single
mistake ("FN-fence mistaken for a lifetime per-document wire-once guard") is the spine of RED-1.

---

## Provenance — two audits (both grounded on live `origin/main` `9e6cf96`)

- **Audit A** — framing-decorrelated, **same-model (Opus)**: verdict `FIX_FOLLOWUP`, 2 RED + 8 YELLOW.
- **Audit B** — **model-decorrelated external reviewer** (run via the standalone brief): verdict
  **`SYSTEMIC`**, 5 RED + 3 YELLOW. **Strictly stronger** — it constructed two P2/P4 counterexamples A
  missed (A even wrongly called `EvidenceDiscriminant` "lossless").
- **Every RED below independently re-grounded against live code by the author** (grounding-gate). Audit B's
  two new SYSTEMIC REDs (RED-1 call-once, RED-2 Accepted payload) are confirmed real; see live anchors.

The double-issue KILL SPINE is *mostly* sound (see "What survived"), but the two SYSTEMIC holes mean rev9
**directly admits P2 and P4** — build is blocked until they are fixed.

| Severity | Count | Nature |
|---|---|---|
| RED (confirmed) | 5 | 2 SYSTEMIC (P2 call-once, P4 Accepted-payload) + -12 scope contradiction + raw-port two-contracts + false -4 claim |
| YELLOW (confirmed) | ~9 | ApplyPlan not concrete-total, auth-UPDATE guards undefined, 2 impl-decisions, MissingStatus, CloseAmbiguous, NS-3 wording, fence-locus, release-gate, §1 line-refs |
| Complementary (Audit A) | 1 | FN-fence OVER-fences definitive rejects (fail-closed brick) — same fix family |

---

## §1 · Confirmed RED

### RED-1 — no per-document lifetime call-once → an accepted doc can be wired a SECOND time (P2, SYSTEMIC)

**Violates:** P2 (wire-count per document ≤ 1), potentially P3.
**Docs:** keystone :27-29 (NS-1 wire-count ≤ 1 per *document*) vs :280 pin weakened to "≤ 1 per *intent*";
dossier :124-133 (FN-fence released after APPLIED clean accept); spec1 :168-169,185 (keeps
`Sent → ErrorRetryable → Sending → wire`); spec4b :357-360 (wire-call never repeated).

**Grounded (live).**
- 032 uniquifies only `(document_id, attempt_no)` (`delivery_reservation.rs:11`), and `call_started_at` is
  a plain nullable column (`032:80`) — **no unique index on `document_id WHERE call_started_at IS NOT NULL`.**
- `delivery_reservation.rs:64-83` deliberately assigns `attempt_no = COALESCE(MAX,0)+1` (MAX never
  regresses; delete forbidden) → a fresh `RESERVED_NOT_STARTED` per attempt.
- The migration-035 fence predicate releases a clean accept (`certainty=SUBMITTED`, `routing_class=NULL`,
  `APPLIED` → predicate FALSE) → FN-fence no longer counts the first reservation.
- `boot_phase.rs` CASes `Sent → ErrorRetryable` when boot `last_chk` returns `NotFound` (":955-1010",
  "tick-2 … re-drive via Pattern B").
- `fiscal_documents.rs:250` still whitelists `(ErrorRetryable, Sending)`.

**Empirical canary (035 predicate):** rows `doc-A|1|OUTCOME_OBSERVED|APPLIED|SUBMITTED` +
`doc-A|2|RESERVED_NOT_STARTED|` — the second reservation for the **same document** inserts successfully.

**Counterexample.** (1) doc-A attempt 1: DPS accepts. (2) apply → doc `Sent`, SFN/seed written, reservation
`APPLIED`. (3) clean-accept releases the FN-fence. (4) later `last_chk` → `NotFound`. (5) Spec #1
`Sent → ErrorRetryable`. (6) next tick creates reservation attempt 2. (7) `authorize_submission` sees a
fresh `RESERVED_NOT_STARTED` → **DPS wired a SECOND time. P2 violated verbatim.**

**Fix (mandatory).**
- migration 035 adds an **independent lifetime call-once guard**:
  `CREATE UNIQUE INDEX ux_delivery_document_ever_started ON delivery_reservation(document_id) WHERE call_started_at IS NOT NULL;`
- `authorize_submission` must ALSO refuse if any historical reservation for this `document_id` has
  `call_started_at IS NOT NULL`. A safe `NotSubmitted` (`call_started_at IS NULL`) may still get a new attempt.
- **Rewrite Spec #1**: `Sent + last_chk NotFound` no longer send-redrives → read-only reconciliation / RMR only.
- The NS-1 pin must count real RPCs per `document_id` **over all history**, not per intent/reservation nor
  only around one reboot.

### RED-2 — the durable record loses the accepted fiscal_number (P4, SYSTEMIC)

**Violates:** P4 (no lost / double doc across crash).
**Docs:** keystone §2A.1 :105-120 (`EvidenceDiscriminant::Accepted` has **no payload**); keystone §2A.2
:148-152 (ApplyPlan MUST always SFN-stamp on Accepted); dossier :153-180 (claims lossless record).

**Grounded (live).** `SentAccepted` carries `fiscal_number: String` (`mod.rs:615-638`), but
`ObservedOutcomeV1` stores only `{certainty, provenance, routing, remote_correlation_id, node_effect,
authorized_generation}` — **no `fiscal_number`** (`mod.rs:1114-1121`). SFN is stamped only in apply, from
the wire response (`set_server_fiscal_no_tx` under `WireDecision::Sent{server_fiscal_no}`,
`stage_send.rs:1750-1755`). No rule pins `remote_correlation_id == accepted fiscal_number`.

**Counterexample.** DPS returns Accepted `fiscal_number = F` → 4-b-i persists `ObservedOutcomeV1` +
`EvidenceDiscriminant::Accepted` (no F) → crash before 4-b-ii → after reboot ApplyPlan must stamp `F` but
the durable record does not contain it → the impl must either (a) leave it PENDING_APPLY forever
(operational doc loss), (b) mark APPLIED without SFN (ledger corruption), or (c) re-wire/probe for it
(conflicts with P2). **P4 not provable.**

**Fix (mandatory).** Make the durable record carry the payload each outcome needs:
`EvidenceDiscriminant::Accepted { fiscal_number: NonEmptyFiscalNumber }`,
`Rejected { verdict: DpsReject, digest: DecodedResponseDigest }`, etc. Prefer `ObservedOutcomeV1::record`
accept the **full sealed evidence** (not just the classified triple), OR persist the full immutable
ApplyPlan at 4-b-i. This is the concrete realization of "lossless durable record".

### RED-3 — CS-3 both kills AND permits a corrective `-12` attempt (P2, contradiction)

**Docs:** keystone :34-36,220-224 (CS-3 = exactly one wire; corrective deferred) vs spec4b :403-409,424-425
(`-12` named a "CS-3 new-attempt edge that CS-3 must additionally lock") vs spec4b :357-360 (wire never
repeated). An implementer strictly following Spec #4B may add a fresh reservation + second wire **within
CS-3**. **Fix:** in Spec #4B, verbatim — *"Bridge/D/E do NOT implement corrective resend. Any new attempt
after `-12` is forbidden. A corrective protocol is a separate future change-set after amending the locked
specs."* (This also subsumes Audit A's YELLOW that keystone NS-3 "remove `continue`" is too late — gate the
MacRecovery class BEFORE `run_mac_recovery`, which already rewrites the chain bytes.)

### RED-4 — the raw port has two incompatible normative contracts

**Docs:** spec4b :222-230 (`submit_raw(...) -> Result<SendResponse, …>`) vs dossier :88-103 (port returns
contract-owned raw `{evidence, diagnostics}`; engine builds `SendResponse` with store-owned `doc_type`).
Not cosmetic: `-2/-15` cannot be split without immutable `doc_type` (`from_server_code` needs it,
`mod.rs:571-596`), and building `SendResponse` in the adapter drops `WireDiagnostics`. **Fix:** write-back
Spec #4B §6 to the dossier contract — `submit_raw -> Result<RawSubmissionObservation, PortBindingMismatch>`
(pure contract DTO, no tonic/prost); only the engine does raw + immutable `doc_type → SendResponse →
classify`. Fix the DP-1 self-contradiction. **Add as the 10th write-back drift.** (= Audit A RED-2.)

### RED-5 — Spec #4B carries a false current-code claim about `-4`

**Doc:** spec4b :62 (+ footer) — "`-4 → DpsError::Transport`; no `-4` arm in `error_routing.rs`."
**Live:** `dto.rs:277-285` → `DpsError::Indeterminate { code:-4, digest, … }`; `error_routing.rs:331` HAS
the arm. spec4b §8 already acknowledges the shipped A seam, so the doc contradicts itself. This flips
provenance (Transport/NoResponse vs Parsed Indeterminate/ParsedDpsEnvelope). **Fix:** delete the stale
claim, cite `dto.rs:277-285`; residual hazard is the `error_routing.rs:331-338` compatibility routing.
(= Audit A YELLOW-3, upgraded to RED as a substantive false code-claim.)

---

## §2 · Confirmed YELLOW

1. **ApplyPlan still not a concrete total matrix.** keystone :146-160 delegates `target_state/audit/probe`
   to "`route_send_result` verbatim", but `route_send_result` takes a `DpsError` **not present in the
   durable record** — so the projection is not computable from the record alone (the same losslessness gap
   as RED-2, generalized). **Fix:** the matrix must enumerate every durable discriminant → exact ApplyPlan,
   then generate the pair-graph test from a separate normative fixture.
2. **The two authorization UPDATE guards are not SQL-verbatim.** keystone :85 says "guarded UPDATE" but
   fixes no `WHERE`; in particular whether overwriting a non-null `active_delivery_reservation_id` is
   forbidden is undefined. **Fix:** write the exact predicates + mandatory negative cases (stale/non-null
   active pointer; reservation no longer RN; binding/hash/doc/FN mismatch; historical `CALL_STARTED` for
   this document; second authorize after first commit).
3. **Two decisions left to the implementer** (not allowed in a locked oracle): dossier :274-276 legacy
   cutover "RMR/HOLD OR pre-deploy empty gate"; dossier :78-82 RetryClass relocation "descope or
   implement". **Fix:** fix legacy cutover as **fail-closed RMR/HOLD** (empty pre-deploy check = optional
   operational prerequisite); explicitly **descope** the relocation OR name the concrete repo API + migration.
4. **spec4b §5 omits the live `MissingStatus` arm** (status==0 → ProbeRequired); "partition completeness"
   is false; a strict build falls status==0 → `UnknownStatus → TransientRetry` (blind-resend-adjacent).
   **Fix:** add `MissingStatus` to §5 + the §2 table.
5. **`CloseAmbiguousCode{Code2,Code15}` unsourceable** — `from_server_code` collapses `-2|-15` to one
   `CloseAmbiguous{digest}` (`mod.rs:594,601`). **Fix:** descope to a single `CloseAmbiguous` discriminant
   (accept the `-2/-15` audit merge) OR thread the raw close-code through.
6. **Slice-E fence-enforcement locus unspecified** (one 4-pre chokepoint vs 7 per-caller guards). Naive
   per-caller misses the `evaluate_er_redrive` redrive edge + the 4 non-`stage_send` seed-writers. **Fix:**
   state the fence is enforced at the single 4-pre `authorize_submission` chokepoint, then separately
   enumerate the non-`stage_send` paths.
7. **`035` + Slice-E boot fail-close co-ship coupling is diffused.** **Fix:** one named release-gate.
8. **spec2 §3 over-claims the `CallStarted` marker as grounded** (`stage_send.rs:1539`) when it is a
   throwaway unpersisted local (`:1562`); self-corrected in §3.2 but the §3 citation should be softened.
9. **keystone §1 stale line/path citations** (`-12` `:1068`→`1082`; seed-writer `:1785`→`1809`; bare
   filenames for callers under `services/offline_sync/` + `services/reconciliation/`). INFO-grade; re-sync.

**INFO.** `-16 OfflineId` "ALERT" (spec4b) vs `NoNodeEffect` (live); misc anchor off-by-a-few-lines.

---

## §3 · Complementary (Audit A) — FN-fence OVER-fences definitive rejects (fail-closed brick)

The FN-fence third clause `(submission_certainty='SUBMITTED' AND routing_class IS NOT NULL)` has no
`apply_state` gate, so a **definitive `TerminalReject`** (`-1/-5/-7..-10/-16`, non-close `-2/-15`) —
non-issued, seed unchanged, FN continues per **D2** — rests in `ux_reservation_active` forever, and
`no_replace` then aborts the next document's reservation INSERT → **register bricks on the first malformed
receipt** (grounded: `mod.rs:955-961`, `033:244,263-266`, `error_routing.rs:341-345`). This is the
*opposite* direction from RED-1 (fail-closed vs fail-open) but the **same architectural family**: the
FN-fence is doing per-document-resolution work it should not. **The convergent fix is the RED-1
architecture** — separate a **per-document lifetime call-once** (on `document_id`, RED-1) from the
**per-FN chain-fork fence**, then relax the FN-fence to release a definitively-resolved seed-unchanged
reject at APPLIED while keeping genuinely-unresolved / issued-unconfirmed (post-SENT → RMR) fenced.
Pin both: after an applied `-1`, the next FN doc's `authorize_submission` SUCCEEDS; a post-SENT
issued-unconfirmed reject STAYS fenced; an already-`CALL_STARTED` document never wires again.

---

## §4 · What survived the attacks (the sound spine — keep)

- Crash after durable `CALL_STARTED` before observation → `SubmittedUnknown`, fence held, zero send
  (spec2 :51-56, keystone :85, dossier :277-282).
- Crash between record and apply → 035 predicate holds `PENDING_APPLY`; a second same-FN *document* does
  not pass (the failure is RED-2 payload loss, not the fence).
- Concurrent same-FN issuance → partial-unique fence + `BEGIN IMMEDIATE` block it until APPLIED.
- Offline→online race → all 7 `stage_send::run` callers + 4 real seed-writers enumerated; grep-confirmed.
- Stale-generation apply → stored-vs-current generation compare (not node-vs-node); stale stays fenced.
- Empty fiscal ID → `OkButNoFiscalNumber → SubmittedUnknown/ProbeRequired/HELD` safe.
- `-12` → dossier correctly requires short-circuit BEFORE `run_mac_recovery` (only the Spec #4B scope
  contradiction, RED-3, remains).

---

## §5 · Minimum-mandatory before D/E can be built (SYSTEMIC gate)

1. **Per-document lifetime call-once marker** (RED-1) + Spec #1 rewrite (`Sent+NotFound` → reconcile/RMR,
   not redrive) + NS-1 pin over full per-`document_id` RPC history.
2. **Lossless Accepted payload** (RED-2) — durable record carries `fiscal_number` (and every audit/probe
   datum) so record→apply survives a crash.
3. **`-12` scope sync** (RED-3) — Spec #4B forbids corrective resend in Bridge/D/E, verbatim.
4. **Raw-port contract** (RED-4) — Spec #4B §6 → `{evidence, diagnostics}` DTO.
Without (1) and (2) the oracle *directly* admits P2 and P4. (3) and (4) prevent an implementer from
building a second wire / mis-terminalizing close-ambiguous. Then close §2 YELLOWs + §3 brick + the false
`-4` claim (RED-5).

**Follow-up structure.** Off a fresh branch/worktree from `origin/main`, spec-first → RED-pins →
minimal-diff → adversarial review. The fence/call-once redesign (RED-1 + §3) is soundness-critical — design
via `arch-planner`, then **adversarially re-audit the fix itself** (a wrong rule re-opens chain-fork or
brick), and re-run the **model-decorrelated external brief** on the corrected oracle before it is trusted.

---

## §6 · Rev-3 remediation re-check (2026-07-19)

The `SYSTEMIC` verdict above remains the verdict on live `origin/main@9e6cf96`. The corrected design is
`docs/CS3_REMEDIATION_DESIGN.md` rev 3; this addendum records its re-check and does not pretend the code
already exists.

### Independent rulings

- **C1 confirmed:** `seed_advanced` is dead and has been removed. `Ok -> Sent` and `Err -> Routed` are
  disjoint, while SFN/seed writes occur only under `Sent`; no reachable routed-reject row can carry the
  claimed seed-advanced fact.
- **C2 corrected:** P2 uses an independent lifetime index, pre-wire query guard, and an INSERT/no-replace
  historical guard. The third guard was added during re-check after an attack showed that allowing a
  fresh RN and refusing only at authorization leaves an orphan active row that bricks the FN.
- **P4 corrected at the storage boundary:** the existing planned evidence discriminant is represented in
  four union-slot columns on `delivery_reservation`, with an exact leaf/axes/effect/payload matrix and
  strict cold hydration. A Rust-only payload is no longer called durable.
- **C3 escalated and fixed in design:** permanent SubmittedUnknown/-12/-6 SQL fences are removed. They
  remain in existing `PENDING_APPLY` + existing STOP_MODE until the existing `reset_stop_mode` operation
  completes an audited operator resolution. Plain reset fails while such a row exists.
- **Sent+NotFound corrected compositionally:** document RMR, node STOP, trace, and audit are one existing
  `with_immediate` envelope. Shift-RMR is deliberately not used because live FSM has no exit.

### Empirical design checks

A migration-035 prototype was applied to a SQLite database produced by every live migration 001–034:

- all 11 legal evidence leaves were accepted;
- NULL digest, named-code-as-UnknownStatus, Accepted correlation mismatch, wrong `-12` routing/effect,
  and post-OO evidence mutation were rejected;
- an attempt-2 INSERT after historical CALL_STARTED was rejected and left no orphan RN;
- after a definitive APPLIED reject, a different next document on the same FN could reserve;
- a PENDING outcome still blocked that next document;
- the 035 non-empty activation guard stopped before any evidence column/index was installed when the
  sqlite client was run fail-fast, matching sqlx's transactional migration behavior.

These are design-prototype checks, not substitutes for the Rust/production teeth listed in remediation
§7.

### Re-check verdict

**Remediation design:** `DESIGN_SOUND, IMPLEMENTATION NOT YET GATED`.

The design now names a concrete defender for P2, P3, P4, and BRICK without adding a table or a domain
aggregate. Four errors were found and fixed during the re-check itself: orphan-RN after historical
CALL_STARTED, offline Accepted seed rollback, unusable `Opening -> Created` operator rollback, and the
absence of any live `BLOCKED` exit after `-11`.

**Live product / D-E merge:** still `NO-GO` until migration, record/hydration, whole-fence, operator
origin×doc-type matrix, and guard-removal RED tests exist in code and pass the full gate.
