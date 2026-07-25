# Coordinated fix DESIGN — active-chain-tip after `NotAcceptedOffline` rewind

**bd:** PRRO_GATE-2nk (P1). **Fixes:** the proven recovery bug (`NotAcceptedOffline` rewind undone by
NC-03 boot) + the invariant_scan cohort-cancel false-positives, coordinated so **boot recovery, the
`last_issued` projection, and `invariant_scan` all agree**.

Repro (RED pin, `#[ignore]`d): `boot_phase.rs::tests::nc03_boot_undoes_not_accepted_offline_rewind`.

---

## 1. Root cause (proven)

`is_issued(RMR-offline) = true` (`OFFLINE_ISSUED_STATES`). After a `NotAcceptedOffline` rewind
(seed→H0, held→RMR, cohort→CANCELLED), the held RMR doc is the highest-lnd issued doc, so BOTH
consumers of the shared `is_issued`-based chain-tip projection pick its hash **H1** instead of the
durable rewind target **H0**:
- `last_issued_unsigned_xml_sha256` → boot NC-03 reconstruction resurrects H1 (BLOCKER 1);
- `invariant_scan` walk → ChainSeedMismatch (the original false-positive).

The projection's own comment pins that it and the scan "CANNOT diverge" — so the fix must be a SINGLE
change to the shared chain-tip notion, applied to BOTH consumers.

## 2. The distinction (the finding's "active-chain vs historical-issued")

- **historical-issued** (`is_issued`, UNCHANGED): "ever crossed OFFLINE_LOCAL_ACK / carries sfn" (M2-01).
  Consumers that must NOT change: Z-quiescence, offline-code backing, legal offline-receipt history.
  The held BEGIN DID locally fiscalize → it stays historical-issued.
- **active-chain tip** (the seed the live MAC chain currently rests at): after a `NotAcceptedOffline`
  rewind the held doc's contribution is VOID (the seed moved below it). Only the CHAIN-TIP projections
  (`last_issued` + scan) must exclude it.

## 3. The marker (why, not RMR-state)

BLOCKER 2: RMR is a GENERAL terminal (`Sent→NotFound`, ER-drain, boot-ER, online `NotAccepted`,
`MacReseed`) — `state == 'RMR'` does NOT prove a rewind. The auditor's directive: key on an EXPLICIT
witness of a completed `NotAcceptedOffline`, not the bare state.

**Design:** an additive, nullable marker on `fiscal_documents`, set ONLY by the `NotAcceptedOffline`
rewind on the held doc. All existing rows keep NULL; no backfill; no behavior change for existing data
(migration-keeper-friendly).

```sql
-- migration NNN
ALTER TABLE fiscal_documents ADD COLUMN chain_superseded_at TEXT;  -- NULL = active; non-NULL = its
-- chain contribution was rewound away by a NotAcceptedOffline completion (historical-issued, NOT the
-- active chain tip). Set once, at the completion; never cleared.
```

## 4. Why `last_issued` EXCLUDING superseded == H0 (the correctness argument)

The rewind target H0 = the held doc's `previous_hash` = (single-writer) the unsigned_sha of the doc it
chained onto = **a surviving issued doc's hash, OR genesis (None) if the held doc is a session BEGIN**.
NC-03 preserves `fiscal_documents`, so that prior doc survives. Therefore:

`last_issued_unsigned_xml_sha256` filtered to `chain_superseded_at IS NULL` returns:
- the prior issued doc's hash = H0 (held chained onto a real prior doc), OR
- None = genesis = H0 (held is the BEGIN, `previous_hash` NULL).

Either way == the durable rewind target. No need to read `previous_hash` directly. (The current repro
seeds an unrealistic dangling `previous_hash` with no producing doc; the realistic repro seeds the
prefix doc so H0 has a producer — see §7.)

## 5. Changes (coordinated)

1. **Migration** (§3) — additive nullable `chain_superseded_at`.
2. **`complete_operator_pending` `NotAcceptedOffline` arm** (`delivery_reservation.rs:1451-1481`):
   alongside the rewind + `doc_to_rmr`, `UPDATE fiscal_documents SET chain_superseded_at = <ts> WHERE
   document_id = held`. In the SAME tx (atomic with the rewind).
3. **`last_issued_unsigned_xml_sha256`** (`fiscal_documents.rs`): add `AND chain_superseded_at IS NULL`
   to the fetch (or filter in Rust). `is_issued` itself UNCHANGED. → boot NC-03 recovers H0 (BLOCKER 1).
4. **`invariant_scan` MAC-walk**: the active chain = issued, `chain_superseded_at IS NULL`, non-dead
   docs; verify continuity + `node_seed == active tip` (the ORPHAN suffix — the superseded held doc +
   its CANCELLED cohort — is excluded from the active `expected`). Keyed on the MARKER (not RMR-state,
   resolving BLOCKER 2) and the dead states. Separately verify the orphan suffix's INTERNAL continuity
   (audit MAJOR — a CANCELLED doc's back-pointer is still a signed historical MAC link) WITHOUT
   advancing the active seed with it. New violation only if the orphan suffix itself is internally
   broken.
5. **Consumers audit** — every `is_issued` / `last_issued_unsigned_xml_sha256` call site: confirm which
   want historical-issued (unchanged) vs active-chain-tip (exclude superseded). Fuzzer model mirror.

## 6. Explicitly out of scope (parallel findings, do NOT conflate)

- **MacReseed** has the SAME class of boot-reconstruction bug (it re-bases the seed to the operator's
  corrected value, ≠ any doc hash; `last_issued` after MacReseed is also wrong). NOT fixed by
  `chain_superseded` (MacReseed's held doc is reseeded, not superseded-to-a-prior-doc). File as a
  parallel bd; different mechanism (the durable seed after MacReseed is not derivable from the ledger
  at all → needs its own durable record). This fix is `NotAcceptedOffline`-only.
- Multi-rewind / multi-session interleavings: the single-rewind reachable case is handled; a
  second independent rewind is covered by the same marker (each superseded doc excluded), but nested
  rewinds where H0 itself points at a superseded doc need a test — add one, adjudicate if RED.

## 7. Test plan (teeth, both directions)

- **`nc03_boot_undoes_...` (the repro)** — REALISTIC: seed a prefix issued doc (unsigned=H0), the held
  doc (prev=H0, superseded), the cancelled cohort. After the fix boot recovers H0 → GREEN. Plus a
  BEGIN-held variant (genesis prev → recovers None). Plus **second-boot idempotency** + **next doc
  signs from H0**.
- **Scan**: cohort-cancel-clean (marker-driven); fork guard (a live issued doc above the seed still
  flagged); M2-N2b (RMR with a LIVE successor — NOT superseded — stays anchoring, clean); real
  ChainBreak / ChainSeedMismatch still caught; orphan-suffix internal break caught.
- **Completion**: the marker is set on the held doc, ONLY on `NotAcceptedOffline`, and the existing
  oc10/oc15 durable state is otherwise unchanged.
- **is_issued unchanged**: a Z-quiescence / code-backing test with a superseded doc still counts it
  historical-issued.

## 8. Risk / invariants

- Additive nullable migration; no backfill; existing behavior byte-identical for NULL rows.
- Completion change is IN the existing rewind tx (atomic; no new network/crypto in the tx).
- `is_issued` untouched → Z-quiescence / codes / legal history unaffected.
- Fork detection preserved (scan still flags a live issued doc above the seed; boot still BLOCKS).
- The marker is set-once, never cleared (immutable like the chain).

---

## 10. As-built + review verdict (2026-07-25)

**Adversarial design review (arch-planner, decorrelated): SOUND for the reachable case.**

- **§4 reachability PROVEN** (the hole the review first flagged, then closed): a later NON-cohort
  issued doc above `held.lnd` is NOT reachable, because (i) the single-active-session partial UNIQUE
  index (`offline_sessions.rs:158,450,519`) makes every offline doc above the held lnd same-session →
  cohort-cancelled; and (ii) the acquire mode gate rejects `StopMode`/`Blocked`/`GoingOnline` BEFORE
  `allocate_next_lnd` (`stage_acquire.rs:303-354, 856`), so while the doc is held no new doc (online
  or offline) gets an lnd. Therefore `last_issued(exclude superseded) == H0` (surviving predecessor,
  or genesis). *Load-bearing on (i)+(ii); if a future change allows multi-session offline or an online
  mint above a held offline lnd, revisit (a durable seed record would then be required).*
- **Consumer audit CLEAN** — exactly 3 prod consumers of `last_issued_unsigned_xml_sha256`, ALL
  chain-tip uses that WANT the exclusion: boot NC-03 reconstruct (`boot_phase.rs:1729`), MacReseed
  guard-B tip (`delivery_reservation.rs:1401-1408` — bonus: without the same exclusion, guard-B would
  reject a legit reseed on an FN with a prior rewind), and the `invariant_scan` walk. `is_issued`
  UNCHANGED. No consumer broken.
- **Atomicity ENFORCED** — the marker UPDATE sits IN the `NotAcceptedOffline` arm, same tx as the
  rewind + `doc_to_rmr` (`delivery_reservation.rs`, adjacent to `doc_to_rmr`). Set-once; ONLY that arm
  sets it (`doc_to_rmr` does not) → a bare RMR state never marks (BLOCKER 2 resolved).

**As-built deviation from §5 — scan walk is MINIMAL, not seed-anchored.** The scan fix is two
`continue` guards on the PRISTINE walk: skip `chain_superseded_at IS NOT NULL` (rewound-orphan) and
skip `CANCELLED` (dead cohort). The pristine `node_seed != final expected` seed check + the per-doc
`previous_hash != expected` ChainBreak are UNCHANGED, so real forks / breaks / stale seeds are still
caught (a live issued doc left above the rewound seed advances `expected` past `node_seed` →
ChainSeedMismatch; verified by `live_issued_doc_above_rewound_seed_is_flagged`). This AVOIDS the
`seed_seen` restructure of the earlier (rejected) scan-only design and its F1/F2 holes entirely.
`ABORTED` stays in the walk (never-issued; a wrongly-chained aborted doc still breaks — F4 preserved).

**Documented follow-ups (NOT in this PR):**
- Orphan-suffix internal continuity (audit MAJOR / F4) — the dead CANCELLED cohort's internal
  back-pointers are not verified (they're voided; not a live-chain hazard). Optional hardening.
- MacReseed parallel recovery bug (§6) — separate bd, different mechanism (reseed value not
  ledger-derivable).
- Fuzzer model mirror of NotAcceptedOffline rewind + NC-03 (INFO) — outside the current alphabet.

**Tests (all GREEN):** `nc03_boot_preserves_not_accepted_offline_rewind` (boot repro, was RED);
`invariant_scan_chain_superseded::{cohort_cancel_marked_state_scans_clean, unmarked_rmr_held_doc_is_not_excused
(BLOCKER-2 canary), live_issued_doc_above_rewound_seed_is_flagged (fork), nested_rewind_two_superseded_docs_scans_clean}`;
all 32 frozen `invariant_scan` tests + M2-N2b unchanged-green.
