# Shift-Context Recovery — PRODUCT FEATURE dossier

**Status:** design locked pending §6 live-probe (DPS Query API URL/availability) + operator ack on §4 surface.
**Author:** architect session (Opus 4.8), 2026-07-12.
**Frame (operator):** *"оформить её функцией продукта"* — a NAMED recovery capability, not a one-off shift-unstick hack. Fits [[project_product_vision]] (moat = fuzzer-proven correctness + **recovery** + audit; recovery is a first-class fleet-control-plane primitive) and productionizes [[project_backlog_operator_recovery]].

---

## 0. Provenance

The stuck-shift live investigation (2026-07-12, `docs/DPS_MINUS8_DATE_AND_SHIFT_RECOVERY.md`) proved: a reseeded gateway can NOT close/operate an adopted shift because it lost the shift's **open-time + turnover + Z-number + offline-session** context — and a Z dated "now" gets DPS `-8` (`дата не відповідає Check.date`). It ALSO overturned the earlier "DPS exposes no shift context" conclusion: FSCO's **Query API** does expose it. This dossier turns that discovery into a product feature.

---

## 1. Feature statement

**Shift-Context Recovery** reconstructs a lost/reseeded shift's context by querying the DPS **from the shift's own documents**, then either **adopts** the shift (so trading + a correctly-dated close resume) or produces a **date-matching Z close**.

**What it unblocks (one feature, four wins):**
1. Close a shift stuck OPEN on DPS after a reseed / DB loss (the live blocker).
2. **24h-shift-limit tracking after reseed** (`opened_at` reconstructed from the earliest doc's `DocDateTime`) — closes the T3 blind-spot ([[project_fiscal_correctness_gaps]] #1).
3. **Z-close on an adopted shift** (turnover + Z-number reconstructed → the Z matches DPS → accepted).
4. A **fleet-recovery primitive** (per-FN disaster recovery = a control-plane capability).

---

## 2. The DPS Query API (the NEW transport surface)

Our current DPS transport is **gRPC-only** (`proto/fiscal_server.proto` = 5 RPCs: sendChkV2 / lastChk / ping / statusRro / infoRro). **None list a shift's documents.**

The needed command lives on the **HTTP JSON command API** (reference `PRRODPS.DFS`, `DPS_REFERENCE_IMPL_ANALYSIS.md`):
- `ApiCmd = .../fs/cmd` — **JSON commands, sent UNSIGNED** (`DFSWebClient.cs:202`).
- **`CmdDocumentsByShiftFiscalNumReq`** — list a shift's documents. Response (per `docs/dps_protocol/263155.md:155`) includes **`DocDateTime`** = date/time of each document (the reconstruction key).
- Companions worth the same client: `CmdRROShiftsByPeriodReq` (shifts in a range → `CloseShiftFiscalNum`/`ZRepFiscalNum`, 263155.md:154), `CmdLastShiftTotalsReq` (last-shift aggregates), `CmdCheckByFiscalNumReq` (a specific doc).

⇒ **New surface = a small HTTP JSON DPS-Query client** (unsigned commands, gzip + TLS1.2 like the check path), added ALONGSIDE the gRPC `DpsChannel` — NOT a `DpsChannel` method (different protocol, different endpoint, unsigned). New trait, e.g. `DpsQueryChannel`.

---

## 3. Reconstruction logic (docs → shift context)

From `CmdDocumentsByShiftFiscalNumReq(open_shift)`:
- **`opened_at`** = MIN(`DocDateTime`) over the shift's docs (the shift-open doc). → feeds the 24h limit.
- **turnover** = aggregate the docs' sums per payform/tax (mirrors `aggregate_z_payload` but sourced from the DPS doc list). → feeds the Z close totals.
- **Z-number** = next in the FN's Z sequence (from `CmdRROShiftsByPeriodReq` last Z + 1). → the `<Z NO=>`.
- **offline-session state** = infer from doc types (BEGIN/END markers) → whether a dangling offline session must be ENDed first.
- **`<TS>`** of the close = within the shift's valid window (≤ `opened_at`+24h, ≥ last doc) — the `-8` fix.

---

## 4. Product surface (operator + automatic) — the "функція продукту"

**(A) Operator command** — explicit, audited, conservative:
`prro doctor --recover-shift --fn <FN>` → query DPS → reconstruct → **present** the reconstructed context (dates/turnover/Z-no) → on confirm, **adopt** (write the `shifts` row + `node_state.current_shift_id` + `shift_state=Opened`) OR **close** (drive a date-matching Z). Never silent-mutate (mirrors the `doctor --repair` safety posture, [[project_backlog_operator_recovery]]).

**(B) Boot-recovery branch** — the reseed/NC-03 adopt point (`boot_phase.rs:1762-1835`) currently sets `ShiftState::Closed` + BLOCKS + surfaces `surviving_open_shift` ("operator MUST reconcile, NOT auto-recovered"). Add a **config-gated** branch: if `recovery.auto_adopt_shift` AND DPS `statusRro.open_shift==true`, reconstruct via the Query API and adopt instead of blind-Closed. **Default OFF** (conservative; auto-recovery of fiscal state is high-stakes).

**(C) Fleet primitive** — the reconstruction fn is per-FN and pure(ish) → reusable by a future fleet control-plane "recover FN" action.

---

## 5. Invariants (state each)
- **No silent state mutation** — reconstruction is a READ; adoption/close is explicit (operator confirm or config flag), audited. Honors the persistence pin (no doc rests non-terminal; the adopted `shifts` row is a legal `Opened`).
- **No network/crypto in txn** — the Query API calls happen OUTSIDE the write txn; only the reconstructed values are written under lease.
- **Single-writer / lease** — adoption writes `shifts`+`node_state` under the FN lease.
- **Fail-closed** — if the Query API is unreachable / the reconstruction is ambiguous (e.g. missing DocDateTime), **do NOT adopt**; fall back to the current BLOCK+surface (RequiresManualReconciliation), never a guessed context.

---

## 6. Risks / unknowns (must resolve before build)
1. **`/fs/cmd` URL for the TEST cabinet is UNKNOWN.** Reference uses prod `fs.tax.gov.ua:8643`; our live gRPC is `cabinet.tax.gov.ua:9443`. **Slice 0 = a live probe** to find + confirm the Query-API endpoint on the test cabinet (read-only). If the test cabinet does NOT expose `/fs/cmd`, the feature is prod-only and the stuck test-shift waits for DPS 24h auto-close.
2. **Allowlist** (default-deny, `live_dps_extended_smoke.rs`) must add the Query-API host explicitly.
3. **Unsigned commands** — confirm the Query API is truly unsigned (reference says so) vs needs the CMS fn_sign; our signer exclusion list (gap #2 in the reference analysis).

---

## 7. Slicing / implementer contract (RED-first)

- **Slice 0 (live probe, read-only):** find/confirm the DPS Query-API endpoint on the test cabinet; capture one `CmdDocumentsByShiftFiscalNum` response for FN 4000162280's open shift → GROUND-TRUTH the wire contract. GATES the rest.
- **Slice 1 (transport):** `DpsQueryChannel` trait + HTTP JSON client (`CmdDocumentsByShiftFiscalNumReq/Resp`, unsigned, gzip+TLS). RED: a scripted-response test → parsed struct.
- **Slice 2 (reconstruction):** pure `reconstruct_shift_context(docs) -> ShiftContext { opened_at, turnover, z_number, offline_state }`. RED: fixture docs → expected context.
- **Slice 3 (operator command):** `doctor --recover-shift` — query → reconstruct → present → adopt/close on confirm. RED: adopt writes a legal `Opened` shifts row + current_shift_id; `assert_clean` holds.
- **Slice 4 (boot-recovery branch, config-gated, default OFF):** the `boot_phase` adopt branch. RED: with the flag + a scripted Query API, reseed adopts instead of blind-Closed; teeth: flag OFF → unchanged BLOCK+surface.
- **Slice 5 (fuzzer):** see §8.
- **Live capstone:** close the actual stuck shift (FN 4000162280) via the operator command → the -8 is gone (date matches).

Each slice: architect verifies gate+teeth, 2-lens review, batch push on operator command.

---

## 8. Fuzzer-impact (mandatory rule [[feedback_fuzzer_tracks_features]])
New recovery op + new wire surface → fuzzer must gain: `Op::ReseedThenAdopt` (lose node_state → reconstruct from a scripted Query API → adopt → the shift closes cleanly, no non-terminal rest, `assert_clean`); and the **envelope/date oracle (RAGE W2)** must pin the reconstructed close's `<TS>` is within the shift window (a `-8`-by-date is caught in one run). Teeth: revert the date-alignment → the seeded harness REDs. Track in `docs/FUZZER_TIER2_RAGE_DOSSIER.md`.

---

## 9. Open questions (operator)
1. **§6.1** — is the Query API expected on the test cabinet, or is this prod-only (→ test shift waits for auto-close)?
2. **§4 default** — boot auto-adopt default OFF (recommended) vs ON?
3. Scope of companion commands now: just `DocumentsByShiftFiscalNum`, or also `RROShiftsByPeriod` (also unblocks the deferred PERIODIC_REPORT, reference gap #5)?
