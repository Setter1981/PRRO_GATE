# CS-3 gap 4b — offline-cohort operator completion: fork-safety design-of-record

**Status:** DESIGN **rev2** (post external review). rev1 got a NOT-YET / "frame is right, three
logical errors" verdict; all three are fixed below and each fix was re-grounded against live code.
For a final GO before code.
**Scope:** the last S7-0 gap — OFFLINE-origin operator completion of a PENDING reservation
(the `OfflineCohortCleanupRequired` + offline-`ShiftFamilyNotSupported` fail-closed stubs).
**Authority:** design `docs/CS3_REMEDIATION_DESIGN.md` §3.4; the architect's 4b directives; a
5-lens adversarial fork-safety pass; and the external reviewer's rev1 corrections. Anchors are
`file:line` in `rust/prro`.

**Why a design gate first:** this is the only S7-0 piece that can violate **P3 (the per-FN
MAC/fiscal chain must never fork)**. gap 4a (online shift-family) is landed and green.

---

## 0. The operation

An offline-origin document's send was ambiguous (SubmittedUnknown / crashed); its reservation rests
`OUTCOME_OBSERVED + PENDING_APPLY` under `STOP_MODE`. Resolving it **not-accepted** rewinds the local
offline chain head — so every **later** offline document chained after it becomes a fork candidate
unless it is atomically cancelled and the chain seed is rewound to exactly the value the current doc
saw. Everything runs in the operator-completion **service** orchestrator's single `with_immediate`
(one `BEGIN IMMEDIATE`/one `COMMIT`, `db/tx.rs:118-160`): shift projection (service) +
`complete_operator_pending` core + Critical audit. `status_rro` is the admin facade's job, OUTSIDE
the tx (invariant #1).

---

## 1. rev1 → rev2 — the three corrections (all verified against live code)

### C-1. Terminal successors are NOT all safe — REJECTED / RMR / ERROR_RETRYABLE are *issued*
rev1 ignored `ACK/REJECTED/RMR` as terminal and called `ERROR_RETRYABLE` non-issued. The single
source of truth `OFFLINE_ISSUED_STATES` (`fiscal_documents.rs:1177-1185`, architect-locked) says the
opposite: an offline-origin doc that EVER reached `OFFLINE_LOCAL_ACK` advanced the local MAC seed
**regardless of later drain outcome** — the issued set is
`{OFFLINE_LOCAL_ACK, SENT, KVT1, KVT2, ERROR_RETRYABLE, REJECTED, REQUIRES_MANUAL_RECONCILIATION}`.
**The break rev1 missed:** current `lnd=10` NotAccept; successor `lnd=11` already `REJECTED`; rev1
ignores it as terminal → the chain still counts `lnd=11` as issued → the next doc chains through a
document signed off the now-repudiated `lnd=10` = **fork**. So a later *issued* successor is a hard
fence, not a terminal to ignore. (Verified: `OFFLINE_ISSUED_STATES` array + the `is_issued`
predicate doc, `fiscal_documents.rs:1177-1205`.)

### C-2. The seed is `current.previous_hash`, not the global `last_issued` tip
rev1 derived the rewind seed from `last_issued_unsigned_xml_sha256(fn)` (the *global* chain tip) and
gated it with an operator-supplied seed + `SeedMismatch`. Both are wrong/unneeded. The seed to
restore is the chain head the current doc *itself* saw — already persisted, immutable, as the
document's own `previous_hash`, pinned from `node_state.last_known_unsigned_xml_sha256` at first
signing (`stage_sign.rs:344`: `let seed = ns.last_known_unsigned_xml_sha256;`). So:
`NotAcceptedOffline` carries **no operator seed**; after the cancels, install
`node_state.last_known_unsigned_xml_sha256 = current.previous_hash`. `Some(hash)` restores the
predecessor, `None` restores genesis — both DB-authoritative, no operator trust. (Verified:
`stage_sign.rs:330-345` pin; `fiscal_documents.rs:98` `DocumentRow.previous_hash`.)

### C-3. Offline Accepted must NOT move the shift forward
rev1 let offline shift-family Accept fire `OLPD→Opened` / `CLPD→Closed` (cohort-gated). Wrong:
Accept only resolves *send*-uncertainty — the doc becomes `Sent`, not yet ACK. The forward edges
`OLPD→Opened` (#5) / `CLPD→Closed` (#13) belong to the drain's `commit_finalize_envelope` and fire
ONLY after the full-cohort ACK drain (`backlog_drain.rs:3090-3101`, "MED-W9B-1 fix"). So offline
shift Accept: doc → `Sent`, mode → `GOING_ONLINE`, shift **stays** `OLPD`/`CLPD`; the normal backlog
drain reconciles and applies edge 5/13 at finalize. (Verified: `backlog_drain.rs:3090-3101`.)

---

## 2. Design-of-record (rev2)

### 2.1 Successor predicate — TX-BOUND, session-scoped *(unchanged from rev1)*
On the caller's `WriteTxConn` (extend the existing doc SELECT at `delivery_reservation.rs:1083-1092`
to also fetch `offline_session_id`, `lnd`, and this doc's `previous_hash`):
```sql
SELECT document_id, state FROM fiscal_documents
 WHERE fiscal_number = ?this_fn AND offline_session_id = ?this_session_id AND lnd > ?this_lnd
 ORDER BY lnd ASC
```
NEVER `offline_sessions::list_pending_for_session` (pool-bound = the one real fork window). Session-
scoping is provably complete (≤1 active session/FN, `offline_sessions.rs:60-62,450`; `lnd` unique per
FN, `001_baseline.sql:352`). Classify in Rust, not via a SQL state filter. Runs FIRST, before any
mutation.

### 2.2 State partition — corrected per C-1 (exhaustive; classify-all-then-mutate)
| later-successor `state` | action |
|---|---|
| `OFFLINE_LOCAL_ACK` | **cancellable** → `transition_state(tx, id, OfflineLocalAck, Cancelled)` (the only `(X,Cancelled)` edge, `fiscal_documents.rs:237`); **require `Applied`** for every one, else rollback (the CAS `WHERE state=OFFLINE_LOCAL_ACK` is the in-tx TOCTOU re-check). |
| `CANCELLED` / `ABORTED` | ignore (truly non-issued / dead). |
| `ACK` / `REJECTED` / `REQUIRES_MANUAL_RECONCILIATION` / `ERROR_RETRYABLE` / `SENT` / `KVT1` / `KVT2` | **hard fail-closed** → `Err(LaterSuccessorIssued{document_id, lnd, state})`, change nothing (all are *issued* — the chain already advanced past current; rewinding current would fork). |
| `SENDING` | **hard fail-closed** → `Err(LaterSuccessorInFlight{document_id})` (mid-wire on the online ladder). |
| `PREPARED` / `SIGNED` / `ENCRYPTED` | **structural error** → `Err(LaterSuccessorInvalidState{document_id, state})` (offline staging rests only at OLA or terminal Aborted, `inline.rs:888-907`; a strict-sequential drain halts at the first non-ACK — cannot legitimately exist). |

**No auto-redrive of ERROR_RETRYABLE, and no "drive it terminal then re-run"** — an offline `RMR`
stays *issued*, so terminalising the ER successor does not clear the fork; it is a genuine chain
conflict the operator must reconcile differently. `OfflineCohortCleanupRequired` is replaced by these
typed errors so the CLI/audit names the blocking class.

### 2.3 Seed policy — corrected per C-2 (rewind to `current.previous_hash`, DB-authoritative)
After the OLA cancels land:
- install `node_state.last_known_unsigned_xml_sha256 = current.previous_hash` via
  `node_state::update_last_known_xml_sha_tx` (`node_state.rs:164-177`; `false`=missing FN →
  `NodeStateMissing` → rollback). `current.previous_hash` is `Option<[u8;32]>` — `Some` restores the
  predecessor, `None` restores genesis (the ONLY sanctioned genesis path);
- **no operator seed, no `SeedMismatch`.** OPTIONAL defence-in-depth (both DB-authoritative, keep if
  cheap): assert `current.previous_hash == last_issued_unsigned_xml_sha256_tx(fn, WHERE lnd < this_lnd)`
  → else `Err(PredecessorMismatch)`; this needs the tx-bound issued-filter and only fires when the
  persisted pin already disagrees with the recomputed predecessor (a corruption tell).

### 2.4 Typed resolution + origin cross-check — corrected per C-2
`OperatorResolution` (`delivery_reservation.rs:937-944`):
```
Accepted{fiscal_number} | NotAccepted (online only) | NotAcceptedOffline (offline only, NO seed) | MacReseed{seed}
```
Origin cross-check at the TOP of the match, DB-re-derived `online = offline_fiscal_no.is_none()`
(`:1093`) as the SOLE origin authority (the variant is an explicit-intent marker, never the origin):
- `NotAccepted` requires `online==true` else `Err(OriginMismatch)` (replaces the
  `OfflineCohortCleanupRequired` stub at `:1138`);
- `NotAcceptedOffline` requires `online==false` else `Err(OriginMismatch)`;
- `MacReseed` gains `if !online → Err(MacReseedNotOfflineDefined)` (§3.4 has no offline MacReseed
  cell; today it blindly `node_advance_seed` for both origins — an existing unguarded fork, closed).

### 2.5 Shift edges — corrected per C-3 (only the two NOT-accepted rollback edges are new)
Two new edges in `shifts::allowed_transition` (`shifts.rs:80-101`), authorized ONLY from
`operator_completion` (sole-caller test, like edge 16):
- `(OpenedLocalPendingDrain, Closed)` — offline SHIFT_OPEN not-accepted (never-opened → Closed);
- `(ClosingLocalPendingDrain, OpenedLocalPendingDrain)` — offline SHIFT_CLOSE/Z_REPORT not-accepted
  (not-closed → back to open pending-drain).

Drift-guard **16 → 18** (`shifts.rs:39` doc + `tests/shift_state_whitelist_matrix.rs`: 18 Applied +
63 Forbidden). The forward edges `OLPD→Opened` (#5) / `CLPD→Closed` (#13) **already exist and stay
the drain's** — operator-completion NEVER emits them. `shift_projection`
(`operator_completion.rs:148-172`) gains `!online` NOT-accepted arms only.

### 2.6 Accepted handling (offline) — corrected per C-3
- offline **regular** Accepted = stamp F + ZERO seed write (the `if online` guard at `:1116` already
  gates it, proven by `oc02`) + doc `Sending→Sent` + mode `GOING_ONLINE`. No cohort guard, no shift
  touch. (No change to the core Accepted arm.)
- offline **shift-family** Accepted = doc `Sending→Sent` + mode `GOING_ONLINE`; shift **stays**
  `OLPD`/`CLPD` (NO forward edge). The backlog drain applies edge 5/13 at finalize. So the
  orchestrator's `shift_projection` returns **None** for offline Accepted (no shift transition), and
  the offline-`ShiftFamilyNotSupported` stub is removed for the Accept path.

### 2.7 Atomic ordering (load-bearing)
**Service** (`operator_completion.rs`, offline): S1 read doc_type/shift_id/offline_fiscal_no
(tx-bound) → S2 if offline shift-family **NOT-accepted**, `apply_shift_transition` the rollback edge
FIRST (require `Applied`) — offline Accept applies NO shift edge → S3 `complete_operator_pending` →
S4 Critical audit (see §2.8).
**Core** (`NotAcceptedOffline`): C1 origin cross-check (fail-closed) → C2 tx-bound later-successor
SELECT → C3 classify ALL (any non-OLA-non-dead → typed fail-closed, nothing mutated) → C4 cancel
every later OLA, assert `Applied` each → C5 install `current.previous_hash` seed (map `false`→
`NodeStateMissing`) → C6 doc `Sending→RMR` (assert `Applied`) → C7 APPLIED CAS (rows==1) → C8 clear
pointer → C9 `STOP_MODE→target` CAS (rows==1 else `NodeNotStopMode`). classify-before-mutate +
seed-after-cancel + mode-CAS-last ⇒ partial cleanup impossible by construction.

### 2.8 P4 audit (added per reviewer)
The Critical audit must be **itemised**, not one generic row: it lists every cancelled successor's
`document_id` + `lnd`, and the restored seed (`genesis` or the hex hash). A single generic audit
does not prove which locally-issued documents were cancelled — P4 (durable, total evidence) needs
the cancellation set on the record.

---

## 3. RED-first test plan (each refuse-path asserts the FULL no-mutation postcondition)
- **oc10** happy: predecessor + 2 later OLA → both CANCELLED, `node_state.seed == current.previous_hash`
  (byte-equal, proves rewind-not-tip), doc RMR, APPLIED, pointer clear, mode GOING_ONLINE; audit
  itemises both cancelled `(document_id,lnd)` + the restored seed.
- **oc11** later `REJECTED` successor → `LaterSuccessorIssued`, **nothing mutated** — the exact rev1
  break; core P3 fork proof.
- **oc12** later `RMR` / `ERROR_RETRYABLE` successors → `LaterSuccessorIssued`, nothing mutated (C-1).
- **oc13** later `SENT`/`KVT1`/`KVT2` → `LaterSuccessorIssued`; later `SENDING` → `LaterSuccessorInFlight`.
- **oc14** later `SIGNED` → `LaterSuccessorInvalidState`, nothing mutated.
- **oc15** genesis: `current.previous_hash == None` + later OLA → seed restored to genesis (NULL),
  cancels land; `None`-tip is the only reset path.
- **oc16** `NotAcceptedOffline` on an ONLINE doc → `OriginMismatch`, no seed overwrite.
- **oc17** plain `NotAccepted` on an OFFLINE doc → `OriginMismatch`.
- **oc18** `MacReseed` on an offline regular doc → `MacReseedNotOfflineDefined`, no blind advance.
- **oc19** offline SHIFT_OPEN NotAccepted → `OLPD→Closed` + cohort cancel + `previous_hash` seed +
  doc RMR + APPLIED, atomic.
- **oc20** offline Z_REPORT NotAccepted → `CLPD→OLPD` rollback + core body.
- **oc21** offline shift-family **Accepted** → doc `Sent` + GOING_ONLINE, shift **UNCHANGED**
  (`OLPD`/`CLPD`), no forward edge (C-3 proof).
- **oc22** atomicity: force mode-CAS 0-rows after cancels → whole tx rolls back (single-envelope proof).
- **oc23** cancel-not-`Applied` pin → hard error + rollback.
- **(optional) oc24** `PredecessorMismatch`: corrupt the pin so `current.previous_hash` ≠ recomputed
  predecessor → fail-closed (only if the optional §2.3 assertion is adopted).
- whitelist matrix → 18 Applied / 63 Forbidden; sole-caller test authorizes both new offline edges
  only from `operator_completion`.

---

## 4. Reviewer's rulings on the four open questions (adopted)
1. **ER successor** — fail-closed (`LaterSuccessorIssued`), **no** auto-redrive (it is issued; a
   chain conflict, not a UX retry).
2. **Genesis** — allowed ONLY as restoring `current.previous_hash == None`, never an arbitrary reset.
3. **Operator seed** — **removed**; the source of truth is the persisted immutable
   `current.previous_hash`. (`SeedMismatch` dropped; optional `PredecessorMismatch` compares two
   DB-authoritative values.)
4. **Offline shift Accept** — supported, but with **no** shift-forward edge; the doc goes `Sent`,
   mode `GOING_ONLINE`, and the drain-finalize owns the shift completion (edge 5/13).

---

*No migration (two shift-whitelist edges = code + drift-guard only). No `delivery_reservation`
table change. The seed rewind reuses the doc's own persisted `previous_hash`; no derivation from the
surviving cohort is needed once C-1's fence guarantees no later issued successor survives.*
