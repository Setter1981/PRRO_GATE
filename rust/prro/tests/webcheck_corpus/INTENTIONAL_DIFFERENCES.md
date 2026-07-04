# WebCheck ⟷ Gateway — Intentional Differences Map (U3 deliverable)

**Unit:** WebCheck Phase-1 **U3** (directed differential replay harness,
`rust/prro/tests/webcheck_replay.rs`).
**Authoritative spec:** `docs/superpowers/specs/2026-07-02-webcheck-ground-truth-phase1-design.md`
§3/U3, §5/A3, §7/CP3.
**Sole WebCheck citation source (U0):** `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md`
(no WebCheck claim below is asserted from anywhere else).

## Purpose

U3 replays the U2 corpus through our real write-path (`inline::run`) and diffs
invariant-level observables against the fixtures. **Our state machine is
deliberately richer than WebCheck's** (§0 of the spec: WebCheck is a
compatibility reference, *not* an oracle of our internals). This document
enumerates every place the differential **legitimately** shows a difference, so
those are not mistaken for a **CP3 finding** (a real divergence = our bug / a
mis-derived U1 rule → triage, never re-bless). Anything the diff surfaces that
is **not** on this list is a CP3 finding.

---

## D-1 — Shift model: WebCheck 2-state vs our 9-state

- **WebCheck:** a 2-state shift (open/closed) enforced by the `DATEEND='NULL'`
  **string** sentinel + `shiftuniq UNIQUE ON shifts(DATEEND)`
  (U0 §3.4; DDL U0 §1.1–1.2). Shift-open/close are `ksef` rows (`DocType 8`/`80`).
- **Us:** the M3b 9-state machine
  (`Created→Opening→OpenedLocalPendingDrain→Opened→ClosingLocalPendingDrain→Closing→Closed/RequiresManualReconciliation/Error`;
  `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`).
- **Harness handling:** `SHIFT_OPEN` / `SHIFT_CLOSE` ops are **not drivable
  commands** in our alphabet — U3 **seeds** an `OPENED` shift as fixture setup
  and drives only `SELL` ops (mirrors the invariant-fuzzer `FuzzCtx`). The
  fixture's shift-boundary ops are therefore not minted on our side.

## D-2 — lnd: per-FN monotonic (ours) vs per-shift `localchecknumber` (WebCheck)

- **WebCheck:** `localchecknumber` is **per-shift** — the `checkcount` trigger
  (`CreateDB.cs:28`, U0 §1.3) does `LastLocalCheckNumber+1 WHERE DocType<>8` and
  **resets to `'0'` every shift** (U0 §4). It is **discarded** by the U2
  lnd-translation.
- **Us:** `lnd` is **per-FN monotonic** (`ux_fd_fn_lnd`, migration `001`;
  `node_state.next_lnd` SSOT, ADR-M3-A1), never reset per shift.
- **Consequence & harness handling:** the fixture's `lnd_sequence` starts at 1
  *within the shift* and mints lnds for the seeded boundary docs (D-1/D-3); our
  driven lnds start at 1 *within the FN* over the SELL subset. U3 therefore diffs
  the driven-subset lnds **offset-aligned** (a single documented constant offset,
  because we seed the boundary docs the fixture minted lnds for) **plus strict
  monotonicity and gap-freeness** — never a raw absolute-value equality. Grounding
  for the translation itself: U0 §4 (dense per-FN over `ksef.ID ASC`).

## D-3 — Seeded boundary/lifecycle docs vs minted docs (`SHIFT_OPEN` / `DRAIN` / `SHIFT_CLOSE`)

- **WebCheck:** the corpus mints `ksef` rows for `DocType 8` (shift-open), the
  drain-class doc, and `DocType 80` (shift-close).
- **Us:** these are **not** receipt-write-path commands — `inline::run` handles
  the `SELL` (receipt) lane; shift lifecycle and offline drain are dedicated
  seams (shift service, `backlog_drain`). Our alphabet has **no `DRAIN` op-type**.
- **Harness handling:** boundary/lifecycle ops become fixture **setup** (seed an
  open shift, an open offline session, offline codes) or the real drain seam
  (D-5); only `SELL` ops are driven. Our `fiscal_documents` holds only the SELL
  docs, so the diff operates on the **driven (SELL) subset** — which equals the
  fixture's `issued_lnds`.

## D-4 — Offline durable state: our Pattern C `OFFLINE_LOCAL_ACK`

- **WebCheck:** offline receipts are issued locally (`offline` lifecycle
  `2`→send-select→`1`, U0 §2/§3.2) and sent to DPS on reconnect (its own MAC
  flow, U0 §3.2/§3.3).
- **Us:** an explicit durable state `OFFLINE_LOCAL_ACK` (M3b Pattern C;
  `LEGAL_INVARIANTS` INV-08..INV-14) precedes the drain.
- **Harness handling:** the corpus `offline_drained` class maps to: our offline
  `SELL` lands `OFFLINE_LOCAL_ACK` (durable, one offline code consumed) → the
  real go-online + AckPath **drain** advances the cohort to `ACK`. So
  **`offline_drained` = OLA→ACK-via-drain**, `fs_mode = OFFLINE`. Issued-set
  membership uses the SSOT `fiscal_documents::OFFLINE_ISSUED_STATES` plus the
  universal `ACK` terminal.

## D-5 — Chain: our recomputed unsigned-XML hashes vs WebCheck's synthetic payload-hash chain

- **WebCheck / corpus:** the fixture `previous_hash_chain` is **synthetic** —
  each link is the prior op's canonical **payload** sha256, recomputed over
  synthetic bytes (U2 §4 hash-recompute). It is a *shape*, not our chain.
- **Us:** our real chain (`fiscal_documents.previous_hash`) is over our
  **unsigned-XML / MAC** domain (per-lane MAC flow, U0 §3), **not** the canonical
  payload domain — so the hex values legitimately differ. The chain **head**
  differs too: the fixture chains from the minted `SHIFT_OPEN`, we seed it, so our
  first driven doc's `previous_hash` is the null seed.
- **Harness handling:** U3 diffs the chain **structurally over our own hashes**
  (length, interior-link presence, no-fork), **never hex-equality** vs the
  synthetic chain, and does not diff the head link. Byte/hash-equality vs WebCheck
  XML is explicitly **deferred (N4)**; the cryptographic chain-linkage SSOT is the
  `invariant_scan` MAC-walk, not this harness.

## D-6 — Cancel: no Cancel op in the Phase-1 alphabet

- **WebCheck:** `offline = -1` marks a **cancelled / rolled-back shift-control
  doc** (U0 §2), excluded from the open/close uniqueness indexes. The
  `cancel_edge` fixture marks its op5 `cancelled`.
- **Us:** our Phase-1 alphabet has **no Cancel / void op** (C2 lives with the
  Phase-2 RETURN tranche, spec §8/N3). We do **not** invent cancel semantics.
- **Harness handling:** `cancel_edge` is replayed **partially** — the online-SELL
  prefix up to the cancel-point is driven and diffed; the cancelled op and the
  post-cancel sells are not driven. The gap-preserving-monotonicity invariant
  (WebCheck consumes an lnd for the cancel and resumes issuance after it) is
  faithfully testable only with a real void op → deferred to Phase-2 RETURN.

## D-7 — `Aborted` terminal (richer state space)

- **Us:** an `Aborted` terminal for orphaned non-terminal docs (bug #192 / boot-
  resume twin P1; migration `025`). WebCheck has no equivalent.
- **Harness handling:** not exercised by the clean corpus (no crash ops in
  Phase-1 U3); listed for completeness of the state-space delta.

## D-8 — Corpus→canonical receipt adapter (U2→U3 impedance seam)

- **Observation:** the U2 corpus `payload_json` is **WebCheck-shaped**
  (`receipt.goods[]` with `price`/`quantity`/`sum`; `receipt.payments[]` with
  `amount`/`payment_type`), while our write-path consumes our **canonical** shape
  (`items[]` with `*_kop`/`quantity_thousandths`; `payments[]` with
  `sum_kop`/`type_code`). `inline::run` fails-closed (`SIGN_INTERNAL`) on the
  foreign shape.
- **Harness handling:** a deterministic field-mapping **adapter**
  (`receipt_to_canonical`) maps the synthetic corpus receipt content into our
  canonical payload before driving. Committed-synthetic content only, no external
  data. The **O3** slice compares this canonical output structurally against the
  fixture receipt (arity + Σitems == total). **Follow-up:** a future U2 revision
  could emit canonical-shaped payloads directly (the sanitizer already owns the
  content); the adapter is the seam until then.

## D-9 — Z report exported-not-replayed; dropped DocTypes 10/12

- **Z:** WebCheck X/Z reports are **out of the Phase-1 replay alphabet** (N3). The
  `z_report` fixture carries `replay:false`; U3 **skips it explicitly** (logged
  `exported-not-replayed`), never silently (A2/MED#8).
- **DocType 10/12:** the U2 sanitizer **drops** these (not in U0 tables, CP1
  decision 2); they never enter the driven sequence.

---

## Citation inventory (every WebCheck claim ← U0 `WEBCHECK_GROUND_TRUTH.md`)

| # | WebCheck claim U3 relies on | U0 anchor |
|---|---|---|
| C1 | `localchecknumber` is per-shift, resets each shift; shift-open (`DocType 8`) does not increment | §4; §1.3 `checkcount` (`CreateDB.cs:28`) |
| C2 | lnd-translation = dense per-FN over `ksef.ID ASC`, `localchecknumber` discarded | §4 |
| C3 | 2-state shift via `DATEEND='NULL'` string sentinel + `shiftuniq` | §3.4; §1.1–1.2 (`CreateDB.cs:624`) |
| C4 | `offline` lifecycle: `0` online, `2`→`1` offline send, `-1` cancelled/rolled-back control doc | §2 (`SQLlite.cs:669/672/1061/1164`) |
| C5 | per-lane MAC/hash flow (online / offline / drain) is WebCheck-internal, not our chain domain | §3.1–3.3 |
| C6 | `DocType` meanings: `8` shift-open, `80` shift-close, `0/1/3/4` sells | §1, §2 |
| C7 | `cancel_edge` "cancelled" ⟵ `offline=-1` rolled-back shift-control doc, not a receipt void | §2 |

Per A0's U0-gate: the reviewing architect cross-checks each row line-by-line
against `WEBCHECK_GROUND_TRUTH.md`; a claim absent there blocks the unit (extend
U0 first). No claim above is sourced outside U0.
