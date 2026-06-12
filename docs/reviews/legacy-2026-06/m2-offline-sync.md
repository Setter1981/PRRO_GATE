# Legacy review — M2 `services/offline_sync/*` + offline_codes

**Reviewer:** Opus 4.8 (hunter). **Branch:** `review/legacy-m2` from `origin/main`
@ 2320d4d (#148 merged). **Lenses:** L1 L2 L5 L6 (+L3 on enumerations).
**Suite anchor:** 1392 passed / 5 skipped. **Code untouched** (docs-only).

Module surface: `backlog_drain.rs` (drain), `kvt2_confirm.rs`, `return_online_probe.rs`,
`backoff.rs`, session lifecycle in `db/repositories/offline_sessions.rs` (+offline_codes),
and the emit-side `services/write_path/stage_offline_ack.rs`.

## Score
FT 1 · HIGH 1 · MED 2 · LOW 1 · NIT 0 · HYPOTHESIS 1. (RT-2/RT-5 known-gaps: OUT-OF-SCOPE.)

---

## CONFIRMED findings

### M2-01 | FT | L1 L6 | offline chain seed never advances within a session
- **file:line:** `stage_offline_ack.rs` (whole `run` — zero `node_state` writes) vs the
  ONLY seed-advance sites `stage_finalize.rs:303` + `boot_phase.rs` (ACK/recovery only).
- **claim:** The MAC seed (`node_state.last_known_unsigned_xml_sha256`, read as
  `previous_hash` at `stage_sign.rs:286`) advances ONLY at ACK-finalize. `OfflineLocalAck`
  has NO writer. So EVERY receipt emitted during one offline session signs with the SAME
  `previous_hash`. Two fiscal-truth failures: (a) **legal chain broken on the wire** — the
  signed offline receipts all claim the same predecessor instead of chaining N→N-1;
  (b) a **2+ receipt offline session is undrainable** — doc#1 finalizes (advances seed),
  doc#2 then fails the chain-continuity guard `stage_finalize.rs:293` → drain hard-aborts
  with `BootError::ReconciliationFailed`, doc#2 stuck at KVT2.
- **repro:** EXPERIMENT (Appendix A, real path: sign→offline-ack ×2, no manual prev) —
  verbatim: both docs `previous_hash=None` (`prev1==prev2`), `node_seed=NULL` after emit;
  drain → `(1,"ACK"),(2,"KVT2")`, `stage 5 chain seed mismatch ... expected None, actual
  Some([07c0…])`; `invariant_scan` after = 1 `ChainBreak{lnd:2}`. (Single-receipt offline
  sessions are fine — `kill_point_matrix::k6` passes; the break starts at receipt #2.)
- **fix class:** SEMANTIC / Fable design. Candidate: advance the chain seed at
  `OfflineLocalAck` (offline IS a local fiscalisation — the receipt is legally issued), so
  each offline doc chains off the prior. Touches stage_offline_ack + invariant_scan walk
  (must treat OfflineLocalAck as a seed-advancing state) + scan/finalize alignment. HOT.

### M2-02 | HIGH | L1 L3 | W12 drain fixtures encode a chain the real path cannot produce
- **file:line:** `backlog_drain_per_doc_loop.rs:355-394` (c4) + `common::seed_w12_finalize_prereqs`.
- **claim:** c4 manually seeds a PER-DOC chain — `seed_w12_finalize_prereqs(doc_a, prev=anchor0,
  unsigned=anchor1)` then `(doc_b, prev=anchor1, unsigned=anchor2)`, i.e. `doc_b.previous_hash
  = doc_a.unsigned_xml_sha256` (comment l.371-372: "its unsigned_xml_sha256 becomes the chain
  anchor for doc B's previous_hash"). The REAL offline path produces `doc_a.prev == doc_b.prev`
  (M2-01). So EVERY multi-doc W12 drain test validates an UNREACHABLE state → the suite is
  green while the FT ships. This is why M2-01 was never caught.
- **repro:** grep-fact above + Appendix A (real path contradicts the seeded chain).
- **fix class:** TEST — rebuild the multi-doc drain fixtures to emit via the real sign path
  (or seed `prev=genesis` for all offline docs) so they fail until M2-01 is fixed. Pairs with
  the M2-01 fix-batch.

### M2-03 | MED | L1 | invariant_scan is blind to the broken offline chain pre-drain
- **file:line:** `db/invariant_scan.rs:165-186` (walk over `unsigned_xml_sha256 IS NOT NULL`;
  seed advances only on `state=='ACK'`).
- **claim:** While the cohort rests at `OfflineLocalAck`, the seed-expectation stays at the
  last ACK for every doc, so all-same `previous_hash` matches `expected` → scan reports CLEAN.
  The ChainBreak only materialises AFTER a drain ACKs the first doc. So during the entire
  offline window (could be hours/days) the on-wire-broken chain passes `invariant_scan` and
  any health gate built on it.
- **repro:** Appendix A — `invariant_scan BEFORE drain: 0 violations`; `AFTER drain: 1
  ChainBreak{lnd:2}`.
- **fix class:** Detect intra-session `previous_hash` collisions among `OfflineLocalAck` docs
  (or fold into the M2-01 seed-advance fix, which makes each prev distinct).

### M2-04 | MED | L2 L6 | doc#2 finalize failure hard-aborts drain, not §16.7 manual-recon
- **file:line:** `stage_finalize.rs:293` `ChainSeedMismatch` → `kvt2_confirm`/drain propagate
  as `BootError::ReconciliationFailed`; cf. `backlog_drain.rs:1091-1107`
  `is_manual_recon_retry_class` (covers TerminalReject/FnConfigError/WrapperBug/MacRecovery/
  OperatorEscalation — NOT a chain-seed mismatch).
- **claim:** The M2-01 doc#2 failure is a hard `Err` that aborts the whole FN drain tick and
  recurs every tick (doc#2 stays KVT2, re-selected by the `IN('…','KVT2')` cohort filter) —
  a permanent fail-loop. It does NOT route through the §16.7 manual-recon families, so the
  shift is not escalated to `RequiresManualReconciliation` and there is no operator-facing
  manual-recon surface for this class.
- **repro:** Appendix A drain error is `ReconciliationFailed` (BootError), not a per-doc
  `manual_recon` verdict; cohort query `fiscal_documents.rs` `…ORDER BY lnd` includes `KVT2`.
- **fix class:** Consequence of M2-01; if M2-01 is fixed the path disappears. If deferred, add
  a chain-seed-mismatch → manual-recon-class mapping so the FN doesn't silent-loop.

### M2-05 | LOW | L3 | drain comment "W7 always stamps offline_session_id" — verify on fix
- **file:line:** `backlog_drain.rs:706` ("No active session → no offline cohort can exist (W7
  always stamps offline_session_id)").
- **claim:** The cohort query filters `offline_session_id = ?`; the "always stamps" assumption
  is correct for the current emit path (`stage_offline_ack` step 7 stamps it), but it is an
  unverified cross-module "always" — if any future offline-emit path forgets the stamp, those
  docs become invisible to drain (silent backlog leak). Currently HOLDS (verified:
  `transition_to_offline_local_ack_tx` always sets `offline_session_id`).
- **repro:** grep — single stamp site (`stage_offline_ack.rs:327`); assumption holds today.
- **fix class:** NIT/doc — add an invariant_scan check "no OfflineLocalAck doc with NULL
  offline_session_id" so the "always" is machine-enforced, not comment-enforced.

---

## HYPOTHESIS (not proven — Fable decides whether to dig)

### M2-H1 | ? | L6 | offline-code pool vs lnd allocator divergence
- `offline_codes.code_lnd` is a SEPARATE pool from `node_state.next_lnd`
  (`offline_sessions.rs:380-413` acquires the lowest unconsumed `code_lnd`). `offline_fiscal_no`
  is stamped from `code_lnd`, the doc's `lnd` from `next_lnd`. Whether these two monotonic
  sequences can diverge (e.g., a code consumed by a doc whose `lnd` ordering differs from
  `code_lnd` ordering, affecting drain `ORDER BY lnd` vs legal code order) is unverified.
  Not chased to avoid over-hunt; flagged for a code-pool-focused pass.

---

## OUT-OF-SCOPE (one line each)

- **RT-2** (Z-report offline code reserve) — known-gap per `docs/reviews/redteam-2026-06-12-adjudication.md`; offline-hardening batch after M2. No fresh hunt.
- **RT-5** (36h/168h offline limits) — CONFIRMED no code enforcement (comments only:
  `backlog_drain.rs:2068`, `backoff.rs:74`, `node_state.rs:185/227`); known-gap per the same
  adjudication, offline-hardening batch. Noted, not re-hunted.

---

## Architect rulings (Fable, 2026-06-12)

**Repro re-run independently (migration-grade, FT requires it).** I re-appended the
Appendix-A probe to `kill_point_matrix.rs`, ran it on `2320d4d`, reverted. Output matches
the dossier **verbatim**: `prev1==prev2` (both `None`), `node_seed_after_offline_emit_is_some=false`,
`invariant_scan BEFORE drain: 0`, drain `ReconciliationFailed{… stage 5 chain seed mismatch …
expected None, actual Some([7,192,24,37…])}` (= doc#1.unsigned `07c0…`), docs after drain
`[(1,"ACK"),(2,"KVT2")]`, `invariant_scan AFTER drain: 1 ChainBreak{lnd:2}`. Mechanism also
confirmed by reading: `emit_check` writes `MAC = &h.previous_hash` into the signed XML
(`xml/mod.rs:321`), and drain's `stage_send::run` sends the **already-signed bytes** —
`signing_ctx` is consumed only on the `-12` MAC-recovery re-sign arm, NOT the happy path. So
the baked-in `previous_hash` reaches the wire as-is. **Wire-corruption claim (a) holds.**

- **M2-01 — ruled FT, CONFIRMED.** The MAC chain must advance once per **issued** document.
  Online issuance = ACK (current). Offline issuance = `OfflineLocalAck` (the receipt is legally
  fiscalised at offline-ack time — drain is deferred *transmission*, not re-fiscalisation).
  Today the seed advances only at ACK-finalize, so every offline receipt in a session signs the
  same `previous_hash` → broken legal chain on the wire AND an undrainable 2+ receipt session.
- **M2-02 — ruled HIGH, CONFIRMED, and it is the ROOT of why M2-01 shipped.** `c4` +
  `seed_w12_finalize_prereqs` hand-seed a per-doc chain (`doc_b.prev = doc_a.unsigned`) the real
  path cannot produce. **Fix-order mandate: rebuild the fixtures FIRST** (emit via the real
  sign→offline-ack path, or seed all offline docs `prev=<current seed>`), watch them go RED,
  THEN implement M2-01. The fixtures are the TDD anchor — a fix landed against the lying
  fixtures would re-bless the bug.
- **M2-03 — ruled MED, CONFIRMED.** Folds into the M2-01 fix (scan-walk change below makes each
  offline `prev` distinct and validated during the offline window).
- **M2-04 — ruled MED, CONFIRMED; disappears with M2-01 BUT keep a defense-in-depth item.**
  Even after M2-01, a future chain-seed mismatch should not silent-loop: add a
  `ChainSeedMismatch → manual-recon-class` mapping (it is NOT in `is_manual_recon_retry_class`
  today) so any such doc escalates the shift to `RequiresManualReconciliation` (§16.7) with an
  operator surface instead of aborting the FN drain tick every cycle. SEPARATE small item,
  lands with the fix-batch.
- **M2-05 — ruled LOW, accept.** Fold into `invariant_scan`: "no `OfflineLocalAck` doc with NULL
  `offline_session_id`" — machine-enforce the `backlog_drain.rs:706` "always stamps" comment.
- **M2-H1 — DIG (short, separate pass).** `code_lnd` (offline code pool) vs `lnd` (next_lnd
  allocator) are independent monotonic sequences; drain orders by `lnd`, the legal offline
  ordinal derives from `code_lnd`. If a session can interleave them out of order, drain's
  `ORDER BY lnd` could transmit in an order that disagrees with the legal code order. Bounded
  pass; not a fix-now.

### M2-01 fix DESIGN (Fable-locked — the hunter's one-liner is INSUFFICIENT)

The dossier's candidate ("advance the seed at `OfflineLocalAck`") is directionally right for the
**sign** side but **breaks the drain side**: if offline-ack advances the seed to the *last*
offline doc, then draining doc#1 (lnd-ASC) hits `stage_finalize.rs:293` immediately
(`ns_seed = doc#N.unsigned ≠ doc#1.prev`) — fails on doc#1 instead of doc#2, i.e. WORSE. The
seed is a single pointer = "hash of the last **issued** doc"; drain must not re-touch it. Full
seam (TDD order, all under the per-FN gate / single-writer invariant):

1. **Fixtures first (M2-02).** Rebuild the multi-doc W12 drain fixtures to the real chain
   (`doc_a.prev == doc_b.prev` for same-session offline, or emit through the real path). RED.
2. **`stage_offline_ack` — advance the seed in the SAME `with_immediate` as the
   `Signed→OfflineLocalAck` transition**, AFTER reading the doc's `unsigned_xml_sha256`:
   first assert `ns_seed == doc.previous_hash` (symmetry with finalize §3 — under the FN gate
   nothing advanced it since sign; a mismatch is a real drift → fail-closed), then
   `update_last_known_xml_sha_tx(seed = doc.unsigned_xml_sha256)`. Now offline doc#2 signs
   `prev = doc#1.unsigned`. The online↔offline boundary stays continuous (a later online doc
   signs off the last offline doc; its finalize advances normally).
3. **`stage_finalize` — skip BOTH the chain-continuity guard (§3) and the seed-advance (§4)
   for offline-origin docs.** Add `offline_fiscal_no` (or `offline_session_id`) to
   `fetch_finalize_inputs_tx`; when non-NULL the doc fiscalised at offline-ack — the seed
   already moved past it, so re-validating/re-advancing is wrong. Still do CAS Kvt2→Ack,
   inbox-DONE, outbox, audit. (Online-origin docs: unchanged.)
4. **`invariant_scan` walk — advance `expected` for ISSUED docs, with the offline rule.**
   Online doc issues at `ACK`; offline-origin doc (offline_fiscal_no non-NULL) issues at
   `OfflineLocalAck` AND stays "issued" through its drain states `SENT/KVT1/KVT2/ACK`. Precise
   rule: advance `expected = unsigned_sha` when `state=='ACK' OR (offline_fiscal_no IS NOT NULL
   AND state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','KVT2','ACK'))`. This validates the offline
   chain DURING the offline window (closes M2-03) and stays correct mid-drain (no false
   ChainBreak when doc#1 is at KVT2 and doc#2 chains off it).
5. **M2-04 defense item** (separate, small): `ChainSeedMismatch` → manual-recon class.
6. **M2-05**: scan check "no OfflineLocalAck with NULL offline_session_id".

Pins: a real-path two-offline-receipt drain → both ACK, distinct `previous_hash`,
`send_chk==2`, scan clean before AND after drain; the online→offline→online boundary
continuity; single-receipt K6 still green. This is hot-zone (write_path + offline + persistence
+ invariant_scan) — Opus implements the locked design, Fable reviews the delta B1-style.
**Not a fence-batch.** Class: the same "fixture encodes aspirational, unreachable state" L3
family as B1-Mismatch / the M1 cleared-then-broken verdicts — the test lie is the load-bearing
finding.

---

## Appendix A — M2-01/02/03/04 repro (throwaway test, run on 2320d4d, NOT committed)

Appended to `tests/kill_point_matrix.rs` (reuses its offline helpers), run, then reverted.

```rust
#[tokio::test]
async fn m2_probe_two_offline_receipts_real_chain() {
    let pool = fresh_pool().await; let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await; seed_offline_code(&pool, 2).await;
    let sign_ctx = det_signing_ctx(); let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    // Receipt 1 + 2 — REAL path (inline::run on an offline node) → OFFLINE_LOCAL_ACK, no manual prev.
    let row1 = seed_inbox_sell(&pool).await;
    { let g = gate.clone().lock_owned().await;
      let o = inline::run(&pool,&pool_secure,&KpStub::new(/*0,0*/),&sign_ctx,&fn_sign,&g,&row1).await.unwrap();
      assert_eq!(o.document_state, DocState::OfflineLocalAck); }
    let row2 = seed_inbox_sell_keyed(&pool, "idem-m2-probe-2").await;
    { let g = gate.clone().lock_owned().await;
      let o = inline::run(&pool,&pool_secure,&KpStub::new(/*0,0*/),&sign_ctx,&fn_sign,&g,&row2).await.unwrap();
      assert_eq!(o.document_state, DocState::OfflineLocalAck); }
    // Observe emit: previous_hash of both docs + node seed; invariant_scan.
    // Then set_node_mode(GoingOnline) + backlog_drain::drain(...) with a Match stub ×2; observe.
}
```

Verbatim stdout (`--nocapture`):
```
M2-PROBE node_seed_after_offline_emit=false
M2-PROBE doc lnd=1 state=OFFLINE_LOCAL_ACK prev_is_none=true  unsigned=Some([07,c0,18,25,…,af])
M2-PROBE doc lnd=2 state=OFFLINE_LOCAL_ACK prev_is_none=true  unsigned=Some([c9,be,13,f8,…,71])
M2-PROBE prev1==prev2 ? true
M2-PROBE invariant_scan BEFORE drain: 0 violations []
M2-PROBE DRAIN ERR ReconciliationFailed { fiscal_number: "4000000001",
  source: stage 5 chain seed mismatch for doc DocumentId(019ebb4c-…):
  expected None, actual Some([7,192,24,37,…,175]) }   // = doc#1.unsigned (07c0…)
M2-PROBE DOCS AFTER DRAIN [(1, "ACK"), (2, "KVT2")]
M2-PROBE invariant_scan AFTER drain: 1 violations
  [ChainBreak { fiscal_number: "4000000001", lnd: 2,
    expected_hex: "07c018255703d7cdb389ce7133d6f06dd7f2da7823613e80ac75c82205f415af",
    found_hex: "<none>" }]
```

Mechanism is general (not genesis-specific): with a prior online ACK leaving seed=X, both
offline docs sign `prev=X`; drain ACKs doc#1 (seed→doc#1.unsigned), doc#2 (`prev=X`) then
mismatches. The experiment instance starts from genesis (seed=None).

---

## Migration-grade re-review (Fable, 2026-06-12) — PR #150 verdict

**Verdict: design implemented faithfully; APPROVED with a fixup (landed on the PR branch).**
Gates re-verified locally on the PR head (nextest 1394 passed / 5 skipped; clippy clean).
9-angle adversarial review + per-candidate verification; tx-routing of every new read confirmed
inside the owning `with_immediate` envelope (`&mut **tx`), seed-advance atomic with the
Signed→OfflineLocalAck CAS.

### F1 — DESIGN AMENDMENT (the locked §step-4 formula was incomplete)

The locked walk rule omitted `ERROR_RETRYABLE` from the offline-origin issued set.  The drain
cohort (fiscal_documents.rs) re-selects
`('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')`; a TransientRetry parks an
ISSUED offline doc in ERROR_RETRYABLE, where the as-designed walk stopped advancing `expected`
→ false ChainBreak on the next doc + false tail ChainSeedMismatch.  The implementer transcribed
the design exactly — the gap is the design's.  Fixed in the fixup commit: set extended, the
redundant `"ACK"` literal dropped from the inner set, clean fixture
`clean_error_retryable_offline_doc_mid_drain_scans_clean` (proven RED against the pre-fix walk).
Rule of record: **the walk's drain-state set mirrors the drain cohort IN-set — keep in lockstep.**

### F2 — M2-05 detection test gap (closed in fixup)

`Violation::OfflineLocalAckWithoutSession` was the only variant of 11 without a `detects_*`
fixture; `detects_offline_local_ack_without_session` added per house norm.

### Ruling — M2-04 contract (supersedes the PR-escalation wording)

The escalation's premises were partially wrong: `ChainSeedMismatch` is a `StageFinalizeError`
(the drain *wraps* it into `BootError::ReconciliationFailed` at backlog_drain.rs:~1879); there
is no ricochet into online_convergence (the error is absorbed at the kvt2_advance / kvt2_confirm
boundary as `ConfirmError::Infrastructure`); the real surfacing sites are 2, not 3-4.  The only
looping surface is the drain's per-tick cohort re-select.  CONTRACT: intercept at the drain's
per-doc finalize error handler (`process_via_w12_kvt2_advance`) — pattern-match
`StageFinalizeError::ChainSeedMismatch` → `DocVerdict::Failed { manual_recon: true }` +
forensics audit payload (expected/actual seed hex); all other finalize errors stay hard
`ReconciliationFailed`.  A dedicated `FailureClass::ChainSeedMismatch` string is approved
(distinct operator surface).  Reference implementation exists (parallel branch
`fix/m2-01-offline-chain-seed`, commit 4271dfa) — cherry-pick + a path test = the follow-up PR.
Residual (documented, not fixed now): the online ladder logs-and-continues per tick on a
hypothetical mismatch — structurally unreachable for online-origin docs in normal flow;
revisit in A2.2.

### Ruling — sibling runtime reads (mechanism): blessed for the FT branch

Same tx / same row / same Option semantics verified.  The ".sqlx full-cache regen" justification
is overstated (per-query hash files; extending a macro = ~2-file diff).  Follow-up mandate
(rides the M2-04 PR): fold the three sibling reads into the `fetch_*_inputs_tx` `query!` macros
and collapse stage_offline_ack's two scalars into one read.

### Ruling — `concurrent_…` → `sequential_two_docs_acquire_distinct_codes`: blessed

The same-FN race is specification-invalid post-M2-01 (the loser fails the step-7b drift-assert);
production is unreachable (FnWriteGate + PK(fiscal_number, code_lnd)); true-concurrency W5 CAS
coverage survives in
`offline_session_code_pool.rs::concurrent_acquire_on_same_fn_returns_distinct_codes`.
No cross-FN test needed — isolation is by table design.

### Refuted candidates (for the record)

M2-05 scan-query index (test-only scale), per-file test-helper duplication (house norm),
DB-column chain pins (the ledger is the legal source of truth), all envelope-routing candidates
(clean).

## M2-H1 dig (Opus, 2026-06-12)

**Hypothesis:** `code_lnd` (offline pool) and `lnd` (next_lnd) are independent monotonic
sequences; drain sends `ORDER BY lnd` while the legal offline ordinal = `code_lnd`; a session
that issues them out of step would put receipts on the wire in an order contradicting the
legal code order. Flagged BEFORE the M2-01 fix existed; this dig is the post-PR#150 landscape.

**Verdict: REFUTED.** `code_lnd` order is FORCED to equal `lnd` order for every offline doc
that reaches the wire, by two independent guards — the per-FN write gate (precondition
unreachable) and the M2-01 drift-assert (fail-closes any out-of-order issuance). The original
flag was valid pre-M2-01; PR#150 closed it as a side effect.

### 1. Allocator map
- `lnd`: `node_state::allocate_next_lnd` (`node_state.rs:324` — `UPDATE node_state SET next_lnd
  = next_lnd + 1 … RETURNING next_lnd - 1`), called at **stage 1** `stage_acquire::run`
  (`stage_acquire.rs:600`), in stage 1's `with_immediate`.
- `code_lnd`: `offline_sessions::acquire_code_tx` (`offline_sessions.rs:385-395`) — picks
  `… ORDER BY code_lnd ASC LIMIT 1` (lowest unconsumed), called at **stage_offline_ack step 6**
  (`stage_offline_ack.rs:318`), POST-sign, in the offline-ack `with_immediate`.
- Both run under ONE `FnWriteGate` hold per request — the whole inline pipeline
  (acquire→sign→offline-ack) is gated as a unit (`fn_gate.rs:1-10`). Distinct `with_immediate`
  envelopes, but no other request for the FN interleaves between them.

### 2. Wire map — the offline ordinal is NOT in the signed bytes
- `offline_fiscal_no` is stamped at offline-ack (post-sign): the ONLY writer is
  `transition_to_offline_local_ack_tx` (`fiscal_documents.rs:396` — `UPDATE … SET state=
  'OFFLINE_LOCAL_ACK', offline_fiscal_no=? … WHERE … state='SIGNED'`). The signed bytes are
  produced at `stage_sign` (earlier stage); `xml/mod.rs` has ZERO references to
  `offline_fiscal_no`. So the code is added AFTER signing → it is NOT cryptographically bound
  to the receipt; it travels as a wire field (`CheckEnvelope` / `stage_send.rs:143` id_offline)
  + local bookkeeping. **Consequence:** the offline-ordinal integrity rests on the gateway's
  serialization (below), not on the signature. (Noted for Fable — not an H1 defect, but the
  ordering guarantee is operational, not crypto.)

### 3. Divergence paths — each refuted
A divergence (doc_A lnd<doc_B lnd but code_A>code_B) requires `offline-ack(doc_B)` to precede
`offline-ack(doc_A)` (codes are handed out lowest-first at ack time). That requires doc_B to be
SIGNED (prev read) before doc_A's offline-ack advanced the seed — i.e. two SIGNED-without-ack
docs sharing a seed. Both layers below block it.

- **(a) refusal/retry.** A per-doc offline-ack refusal that lets a sibling succeed needs an
  abnormal refusal (`DocStateConflict`/`DocNotFound`/`CrossFnMismatch`,
  `stage_offline_ack.rs:117-146`); the FN-wide refusals (`NodeNotOffline`/`ShiftNotOpened`/
  `NoActiveSession`) hit every doc equally. Even granting it: the lapped lower-lnd doc's retry
  hits the drift-assert (`stage_offline_ack.rs:363-369`, `ns_seed != previous_hash`) and is
  REFUSED a code — it never gets an out-of-order `offline_fiscal_no`. Proven by the probe below.
- **(b) boot re-drive.** Boot routes SIGNED offline docs to `stage_offline_ack::run` via
  `dispatch.rs:156` (drift-assert applies); the pending list is `ORDER BY lnd`
  (`fiscal_documents.rs:473`, `list_pending_for_fn`). AND ≤1 SIGNED-without-ack doc can exist
  per FN: the gate holds acquire→sign→offline-ack as a unit, so a crash strands at most the one
  in-flight doc (the next never acquired `lnd`). One doc ⇒ no reordering.
- **(c) mixed online→offline.** Online docs consume NO codes (the pool is offline-only,
  `acquire_code_tx` is offline-ack-only); `lnd` growth on online docs does not perturb
  `code_lnd` ordering among offline docs. The online→offline seed boundary is continuous and
  drift-asserted (pinned: `kill_point_matrix::m2_online_offline_boundary_chain_continuous`).
- **(d) idempotent replay / double POST.** A replay of an issued doc is refused
  `DocStateConflict` (doc no longer SIGNED) BEFORE `acquire_code_tx` (`stage_offline_ack.rs:244-255`
  precedes step 6) → I5 code-pool-untouched → no new code, no new `lnd`.
- **(e) exhaustiveness.** Only writer of `offline_fiscal_no` = `transition_to_offline_local_ack_tx`
  (grep, §2) → the drift-assert in `stage_offline_ack` guards EVERY code stamp. No
  admin/recovery/migration bypass. The only `ORDER BY code_lnd` is the lowest-unconsumed pick
  (§1); no reader sends/orders the wire by `code_lnd` — drain + boot + scan all `ORDER BY lnd`.

### 4. M2-01 linkage (the post-fix lens)
Post-PR#150 the MAC chain links in ISSUANCE order: `stage_sign` reads the seed
(`stage_sign.rs:286`) into `previous_hash`; the seed advances ONLY at offline-ack
(`stage_offline_ack.rs:383`) for offline docs and at finalize (`stage_finalize.rs:315`) for
online. The drift-assert requires `ns_seed == previous_hash` at ack, so a doc can ONLY be issued
when the seed equals what it read at sign — i.e. offline-ack order is forced to equal sign order,
and sign order equals `lnd`-acquire order (both under one gate hold). Therefore issuance order =
`lnd` order, and since `acquire_code_tx` hands codes out lowest-first at ack time,
**`code_lnd` order = issuance order = `lnd` order**. (i) The "out-of-order" precondition is
unreachable by construction (gate nesting), and (ii) if synthetically created it is
fail-closed (probe). `invariant_scan` walks `ORDER BY lnd` and validates the chain by
`unsigned_xml_sha256`/`previous_hash` (`invariant_scan.rs:173-212`); it does NOT inspect
`code_lnd` ordering at all — it does not need to, because the write-time drift-assert prevents
any out-of-order code from being persisted.

### Repro (throwaway, run on 62f6b54, reverted — NOT committed)
Seeded the worst case (two SIGNED docs, both `prev=NULL` genesis, codes 1+2), offline-acked the
HIGHER `lnd` first:
```
M2H1 r_b(lnd=2)_ok=true r_a(lnd=1)=Err(stage_offline_ack: chain-seed drift for doc … —
  node_state seed != doc.previous_hash (offline issuance must extend the chain from the last
  issued doc); refusing to advance)
M2H1 doc_b(lnd=2): state=OFFLINE_LOCAL_ACK offline_fiscal_no=Some(1)
M2H1 doc_a(lnd=1): state=SIGNED                offline_fiscal_no=None
M2H1 codes: [(1, Some(<doc_b>)), (2, None)]
```
The lower-`lnd` doc is REFUSED a code (drift-assert) rather than handed code 2 — the divergence
never reaches the wire. (The probe even had to seed an unreachable two-prev-NULL state to attempt
it; §3(b) shows the gate forbids that state live.)

### Out-of-scope observations (one line each, no dig)
- A SIGNED doc "lapped" by a sibling would be STRANDED by the drift-assert (liveness, not
  wire-ordering) — but the lapping precondition is itself unreachable (§3(b) gate nesting);
  noted only for completeness, severity LOW/theoretical.
- The offline ordinal not being in the signed bytes (§2) is a property worth a one-line note in
  the crypto/wire review, not an H1 finding.
