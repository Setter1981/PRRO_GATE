# B10 — Offline-Session Drain Handshake (DocType 9/10) — Implementer Dossier

**Author:** architect session · **Date:** 2026-07-08 · **Model for implementer:** Opus 4.8, worktree isolation, **strict RED-first TDD**.

## 1. Intent

Clear the LIVE DPS `-5` "offline id in online mode" hit on `live_smoke_9_offline_drain`. Root cause (verified): on drain we send offline docs (carrying `<MAC ID='{code}'>`, B9) to DPS **without first opening an offline SESSION on the DPS side**. DPS has no open offline window → rejects each offline doc as "offline id in online mode".

**Fix (offline-session handshake on the drain channel):** bracket the drained offline docs with two boundary documents on the existing `sendChkV2` channel:
- **DocType=9 — offline-session BEGIN** ("документ ухода в офлайн"), wire `<C T="109">` (verAPI=2), sent **FIRST**.
- **DocType=10 — offline-session END**, wire `<C T="110">`, sent **LAST**.
- Both `typCheck=ServiceChk(3)`, both offline docs (carry a pool code + `<MAC ID>`).

**Ground truth:** WebCheck `SendingOfflineChecks.cs` (`EndOfflineXML`, `CloseOfflineDoc`) + `Dispatch.OfflineToOnline`. DPS computes offline state + the 168h budget FROM these 9/10 docs (docs-as-canon) — so they are **durable timestamped fiscal documents**, not drain-time ephemera. Verified byte-stable on fresh WebCheck v6.0.8.1368 (`docs/webcheck_reverse_v2/FRESH_WEBCHECK_ANALYSIS.md §1`).

## 2. Agreed design (co-designed with operator; do NOT re-derive)

- **DocType=9 is minted LAZILY as the FIRST offline doc of a session.** Operator's exact framing: "при попытке пробития нового чека офлайн проверяем наличие документа ухода в офлайн; если нету — берём из очереди офлайн-номер и делаем документ ухода офлайн [первым]". So: when an offline **business** doc is about to be minted and the active session has **no** DocType=9 yet → mint the 9 first (consume a pool code, MAC ID, durable OLA), stamped with the offline-entry time (session `opened_at`). It becomes the lowest-`lnd` offline doc.
- **DocType=10 is minted at drain END** (operator: "в конце"; WebCheck `EndOfflineXML` stamped `CurrentCompDate`). Consumes a pool code; sent as the last offline doc.
- **Drain order is natural:** `backlog_drain::drain` iterates `lnd ASC`. The 9 (lowest lnd) drains first; content follows; the 10 is minted+appended at drain finalize and sent last.
- These are **real `fiscal_documents` rows** (durable, MAC-chained, crash-recoverable) — NOT synthesized on the wire only.

## 3. Exact seams (file:line — current reality)

| # | Seam | File:line | Change |
|---|---|---|---|
| S1 | DocType enum | `db/models/enums.rs:100` `str_enum!(DocType{…})` (9 variants) | Add `OfflineSessionBegin => "OFFLINE_SESSION_BEGIN"`, `OfflineSessionEnd => "OFFLINE_SESSION_END"` |
| S2 | XML C-tag build | `xml/mod.rs` (ShiftOpen→`<C T="108">` service-receipt: header+`DI`+`<O>`, no `<P>` goods; asserts at :1714/:1781; Z has NO `<C>`) | Emit `<C T="109">` (begin) / `<C T="110">` (end), service-receipt shape modelled on ShiftOpen(108). **Byte-pin body against WebCheck** `SendingOfflineChecks.cs::EndOfflineXML` + begin builder |
| S3 | Signed-XML kind | `stage_sign.rs:59` `enum WireArtifactKind{ShiftOpen,Sell,Return,ZReport}` + DocType→kind map | Add `OfflineSessionBegin`/`OfflineSessionEnd` arms |
| S4 | Offline code + MAC ID | `stage_sign.rs:939-961` (`current_active_session_id_tx` + `acquire_code_tx` consume a pool code per offline doc) | 9/10 flow through the SAME acquire→OLA path (each consumes a code; MAC ID per B9) |
| S5 | typCheck + id_offline | `stage_send.rs:~394` (typCheck map: ShiftOpen→ServiceChk(3), Sell/Return→Chk(1), ZReport→ZReport(2)) + `:471` `id_offline` | Map 9/10 → `ServiceChk(3)`; `id_offline` = their offline code |
| S6 | Lazy-mint-9 trigger | offline-doc mint path (write-path acquire/sign for an offline doc); session state via `offline_session.rs` / `offline_sessions` repo | Before minting the first offline business doc of a session (no existing DocType=9 row for `offline_session_id`), mint the 9 first. Idempotent existence check |
| S7 | Drain-end-10 | `backlog_drain.rs::drain` (per-doc `lnd ASC`) + `offline_session.rs::start_drain` (Open→Draining) / Draining→Closed finalize | At drain finalize (after last content doc, before Draining→Closed), mint DocType=10 + send it last. Idempotent (re-drain after crash must not double-mint/double-send) |

## 4. Blast radius — new DocType variants (🔴 KEY CORRECTNESS ITEM)

Adding two `DocType` variants affects match sites across ~16 files. **Two classes:**

**(a) Exhaustive matches (no `_`)** — compiler BREAKS the build → safe, implementer fills each arm. e.g. `signer_guard.rs` (0 catch-alls), `xml/mod.rs` C-tag match.

**(b) 🔴 `_ =>` catch-all matches — SILENT-SWALLOW HAZARD.** These do NOT break the build; the new variants fall into the default arm and may be mis-handled. **Implementer MUST inspect every `_ =>` arm on a DocType match and decide explicitly** (add arm vs. confirm default is correct). Files with DocType matches AND catch-alls (upper-bound counts; confirm which `_` are on DocType):
- `stage_acquire.rs` (17 DocType-refs, 8 `_=>`) — guard table. **9/10 are internally-minted, NOT ingress-driven** → decide: do they even enter `stage_acquire`? If yes, need guard arms; if minted via a separate seam, confirm they bypass.
- `stage_send.rs` (16, 6) — typCheck map (S5) must be explicit, not defaulted.
- `inline.rs` (2, 5) / `inline_map.rs` (2, 7) — InlineWritePath doc mapping.
- `stage_offline_ack.rs` (8, 1), `stage_sign.rs` (5, 2).
- Ingress: `convert.rs` (20), `dto.rs` (9), `canonical_builder.rs` (4), `replay.rs` (6), `z_builder.rs` (2), `server.rs` (1) — **9/10 are NOT ingress doc types** → ingress should REJECT/not-accept them from external protocols (they're gateway-internal). Add explicit reject or confirm they can't be built from ingress.
- `fiscal_documents.rs` (2), `boot_phase.rs` (2), `error_routing.rs` (42 — mostly param threading/tests; verify -2/-15 close-shift doc_type logic unaffected).

Deliver a table in the PR: each DocType match site → arm added / default-confirmed-correct / N/A.

## 5. RED tests first (pin these; watch each fail for the RIGHT reason)

1. **`<C T="109">` / `<C T="110">` emission** — DocType 9 builds service-receipt XML with `<C T="109">` (no `<P>` goods); DocType 10 with `<C T="110">`. (mirror `xml/mod.rs` :1714/:1781 asserts.)
2. **typCheck** — `stage_send` maps DocType 9 and 10 → `ServiceChk(3)`.
3. **MAC ID present** — 9/10 signed XML carries `<MAC ID='{offline_code}'>` (B9 path), each consumed a pool code.
4. **Drain ordering (headline)** — an offline session with content `[SHIFT_OPEN, SELL]` drains in wire order **`[9, SHIFT_OPEN, SELL, 10]`**. 9 first, 10 last.
5. **Lazy-mint idempotency** — minting a 2nd offline business doc does NOT mint a 2nd DocType=9 (existence check); a session with zero offline business docs mints NO 9 (no spurious marker).
6. **Timestamps** — 9 stamped with session `opened_at`; 10 stamped at drain close (docs-as-canon / 168h fidelity).
7. **Crash-idempotent drain** — re-running drain after the 9 was already sent (KVT-advanced) does NOT re-send/re-mint it; 10 minted exactly once.
8. **🦷 TEETH** — revert the 9-emission (drain sends `[SHIFT_OPEN, SELL, 10]`) → test #4 FAILS on missing-leading-9. Prove the ordering test has teeth.

## 6. Invariants preserved (state in PR)

1. **No network/crypto in SQLite write-tx** — mint 9/10 rows + code-acquire in short txns; sign/send outside the tx (existing write-path discipline).
2. **Single-writer per FN** — 9/10 minted under the same FN lease as the session's other offline docs; no second writer.
3. **Idempotency (mandatory)** — lazy-9 existence check + drain-end-10 once-only guard; re-drive/re-boot safe.
4. **Offline respects code limits** — 9/10 CONSUME pool codes → see §7 reserve coupling. No code = cannot mint the boundary → must fail-closed, not skip the handshake.
5. **docs-as-canon** — 9/10 are durable `fiscal_documents` rows (offline state), MAC-chained; DPS + we count offline/168h from them.
6. **D2 / advance-at-SEND** — 9/10 are OFFLINE docs → NO online chain-seed advance, NO `server_fiscal_no` online semantics; they advance the OFFLINE code ledger only. Confirm no accidental online-seed touch.
7. **Recovery does not violate transitions** — drain finalize (Draining→Closed) still gated by `finalize_eligibility`; the 10-mint must not bypass the KVT1-deferred / AckCountMismatch guards.

## 7. Named residuals / risks (NOT blocking B10, but state them)

- **🔴 Code-reserve coupling** — the DocType=10 needs a pool code reserved BEFORE drain (codes are only acquirable while offline). If the pool is exhausted at drain, the session cannot be closed → stuck session. This is exactly [[project_backlog_offline_code_reserve_floor]] ("пиздец важно"). **B10 assumes codes are available; the RED test provides enough (≥ content + 2 boundary codes).** The proactive reserve-floor (gate SELL when codes ≤ floor, always reserve for boundary/Z) is the SEPARATE follow-up that GUARANTEES this. Note in PR; do not fold in.
- **B11 (auto-offline entry)** — B10 assumes the offline entry already happened (manual `go_offline` / smoke-forced). The auto-fallback-on-send-fail trigger is separate.
- **Smoke 9 harness** — `T=112 SIZE=2` is now INSUFFICIENT (needs codes for 9 + content + 10). Bump SIZE in the smoke.
- **9/10 XML body exact fields** — byte-pin against WebCheck decompile (`docs/webcheck_reverse_v2/WebCheck/SendingOfflineChecks.cs`); model on ShiftOpen(108) service shape. Do not invent fields.

## 8. Verification

- Targeted: `cargo nextest run -p prro` on the touched write-path/xml/drain tests + the new B10 tests. Full gate green.
- `cargo fmt` + `clippy` clean.
- LIVE (operator-run, gated `PRRO_LIVE_DPS=1` + JKS): re-run `live_smoke_9_offline_drain` — **must clear `-5`**. This is the acceptance gate (task #6).

## 10. End-10 mint mechanism — VERIFIED CONTRACT AMENDMENT (architect-endorsed 2026-07-08)

**Snag (verified against main 1d12ebf):** the DocType=10 is minted at drain **finalize**, when the node is already `GoingOnline` and the session is `Draining`. The standard offline-code path `resolve_offline_dps_code` (`stage_sign.rs:905`) then hits **two** independent blocking gates:
- gate (2) `stage_sign.rs:928-934` requires live `NodeMode ∈ {Offline, GoingOffline}` → under `GoingOnline` returns `OnlineShaped`;
- gate (3) `stage_sign.rs:939` → `current_active_session_id_tx` is **strict `state='OPEN'`** (`offline_sessions.rs:457`) → session is `Draining` at finalize → returns `None` → `OnlineShaped`.

⇒ the naive `run_staged` path signs the 10 with a **bare `<MAC>`** (no offline code) — exactly the `-5`/`-9` bug. **Do NOT widen the shared `resolve_offline_dps_code`** (leaks to every SELL/RETURN AND is insufficient — two gates). Do NOT mint the 10 earlier (loses the close-time stamp + sent-last ordering).

**Resolution — drain-scoped helper `mint_and_drain_session_end(...)` in `backlog_drain.rs`, called from `finalize_drain`'s Eligible arm before `commit_finalize_envelope`, three envelope-bounded steps:**
- **STEP A (mint PREPARED, one `with_immediate`):** `insert_prepared_tx` `fs_mode='OFFLINE'`, `doc_type=OfflineSessionEnd`, `shift_id`=the ClosingLocalPendingDrain shift, `business_ts`=drain-close, `lnd=allocate_next_lnd` (→ highest ⇒ drains last by construction). D2: no `server_fiscal_no`, no online seed. **Idempotency predicate:** `(fiscal_number, shift_id, doc_type=OFFLINE_SESSION_END, state NOT IN terminal-fail)` — if a 10 already exists in {PREPARED…ACK}, SKIP mint, adopt, route by its state.
- **STEP B (sign + acquire code + `<MAC ID>` + OLA seed-advance) — the gate bypass:** call `acquire_code_tx` **directly** (it does NOT gate on node mode) with the `session_id` supplied **explicitly** (held from drain-start), mode-check skipped. Preferred: extract the acquire+stamp+seed-advance core shared by `resolve_offline_dps_code` (`stage_sign.rs:959-984`) + `stage_offline_ack` step 7b (`stage_offline_ack.rs:425-466`); fallback: a small `resolve_offline_dps_code_forced(tx, doc, fn, session_id)` twin — **do NOT widen the shared fn**. Sign OUTSIDE the tx (INV-1). The 10 chains `previous_hash = last drained content doc's unsigned sha`; its own OLA advances the seed to close the offline chain.
- **STEP C (send last + confirm):** `stage_send::run` reads the stamped `offline_dps_code` → `id_offline`, typCheck=ServiceChk(3), offline-shaped, on the drain channel, after the last content send. Drive Sent→Kvt1→Kvt2→Ack via the drain's own `kvt2_confirm` (offline-scoped → no online-seed touch, D2). Only on 10-ACK proceed to `commit_finalize_envelope` (Draining→Closed); if it holds → `OFFLINE_DRAIN_PARTIAL`, session stays Draining, re-enter next tick (existence-check adopts, no re-mint).

**Crash-idempotency:** STEP-A predicate + row-state routing cover crash at PREPARED/SIGNED/OLA (resume B/C), SENT-not-ACK (drain-confirm Sent-replay), 10-ACK-before-close (predicate matches → skip mint+send → straight to close). Wire ensure-10 into **both** `FinalizeEntry::NormalEligible` **and** `CrashRecovery` (`backlog_drain.rs:2633`).

**Teeth (RED):** mint the 10 under live `GoingOnline` → assert signed XML carries `<MAC ID='{code}'>` (NOT bare `<MAC>`) + consumed exactly one pool code (that's the exact naive-path bug). Plus: re-drain after 10-ACK consumes no 2nd code (INV-5).

**Asymmetry (PR line):** the 9 rides `inline::run` (minted while Offline → gates pass); the 10 uses the drain-local helper (minted while GoingOnline → gates structurally wrong). Intentional. Recorded in memory `project_b10_offline_boundary_docs`.

## 9. Delivery format (required 7 items)

Intent completed · Files changed · Tests/checks run · Result · Known risks/not-done · Invariant check (§6) · Suggested next step. Plus the §4 blast-radius table.
