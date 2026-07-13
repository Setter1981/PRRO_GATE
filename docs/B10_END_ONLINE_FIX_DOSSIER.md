# B10 END-online Fix — Implementer Dossier (RED-first)

**Author:** architect · **Date:** 2026-07-09 · Stacked on the B10 branch `worktree-agent-a8e9bc72a98df18bc` (HEAD `b2814d4`). Hot-zone (`write_path` / offline drain / `stage_sign` / `stage_send`). STRICT RED-first TDD.

## 1. Intent — the confirmed root cause

Live-verified: the drain-minted **DocType=10 OFFLINE_SESSION_END was built OFFLINE-shaped** (`<MAC ID='{code}'>` + a consumed offline pool code + a non-empty wire `id_offline`, via `resolve_offline_dps_code_forced`). DPS **terminal-rejects that with `-5 "no id offline"`** on a drain-to-online close, which left a dangling offline session on the cabinet.

WebCheck's drain close (`SendingOfflineChecks.cs::CloseOfflineDoc`, called by `Dispatch.OfflineToOnline`) sends the DocType=10 END as an **ONLINE-shaped** doc:
- **bare `<MAC>{previous_hash}</MAC>`** — NO `ID=`, NO offline code;
- wire submit with **NO `id_offline`** (`SubmitCheck(file, localNum, typCheck=3, date)` — 4 params);
- saved via `SaveXMLcheck` (online), NOT `SaveXMLcheckOffline`.

**Live proof:** a hand-built bare-`<MAC>` online END (recovery `b2814d4`) was **ACCEPTED by DPS (`server_fiscal_no=QBFpNrirZjU`, `online→true`)** and closed the dangling session — with NO offline code. So the fix is: **make the drain-minted DocType=10 END ONLINE-shaped.**

## 2. The change

The END is minted at **drain-finalize** (node `GoingOnline`, session `Draining`) — at that point you ARE going online, so the END rides as an **online issuance**, not an offline doc:
- **XML:** `<C T='110'>` with a **bare `<MAC>{previous_hash}</MAC>`** (online MAC path), NOT `<MAC ID='{code}'>`.
- **Wire envelope:** `typCheck=ServiceChk(3)`, **empty `id_offline`**, `local_number`/DI as an online doc.
- **No offline code:** the END does NOT call `acquire_code_tx` / does NOT consume a pool code. Remove/bypass the `resolve_offline_dps_code_forced` path FOR THE END (that whole drain-scoped offline-code mechanism was built on the wrong premise that the END needs an offline code — it does not).
- **Online issuance semantics:** the END goes through the normal **advance-at-SEND** path — at the `Sending→Sent` CAS it advances the ONLINE chain seed + stamps a `server_fiscal_no` (like any online doc). It chains off the last content doc's tip.

**BEGIN is UNCHANGED** — it stays offline-shaped (`<MAC ID='{code}'>`, consumes a code), because it is minted while OFFLINE and opens the session. Only the END (minted at drain-finalize, going online) flips to online-shaped. This BEGIN(offline)/END(online) asymmetry mirrors WebCheck (`CloseOfflineDocOffline` vs `CloseOfflineDoc`) and is intentional.

## 3. 🎯 D2 / advance-at-SEND (the subtle, review-critical part)

The END becomes a **real online issuance at drain-finalize**. State how each holds:
- **advance-at-SEND:** the END advances the online seed atomically with `server_fiscal_no` at the `Sending→Sent` CAS (that CAS is the issuance moment). After the END, the online tip = the END's hash.
- **pre-SENT reject** of the END → `Sending→Rejected` (non-issued rests, seed NOT advanced) — D2.
- **post-SENT reject / ambiguous** → `RequiresManualReconciliation` (seed NOT rolled back), NEVER `Rejected` — D2 expanded. (This is exactly the edge-12 Z/close ambiguous-timeout family.)
- The END chains `previous_hash` off the last drained content doc; verify the drain finalize computes this from the settled state (the live recovery had to settle the DPS tip — confirm the drain's END uses the correct post-content tip).
- The offline session closes (`Draining→Closed`) only on the END's ACK; a held/rejected END → the existing PARTIAL/re-drive path, but now with online-doc semantics (RMR on post-SENT-ambiguous, not an offline hold).

## 4. Seams (verify against actual code)

- `stage_sign.rs` — the END currently routes through `resolve_offline_dps_code_forced` (~:938/1012). Change: the END must NOT force an offline code; it signs online-shaped (bare `<MAC>`). Likely: route the END doc-type through the ONLINE sign path instead of the forced-offline path.
- `xml/mod.rs` — `emit_offline_session_boundary` (:876) emits the END with the offline MAC. The END now needs `<C T='110'>` + a **bare online `<MAC>`**. Add/route to an online-MAC variant for the END (the BEGIN keeps the offline `<MAC ID>`).
- `stage_send.rs` — the END's envelope `id_offline` must be EMPTY (~:471/477 is where offline docs set it); the END now takes the online advance-at-SEND branch (the `if offline_fiscal_no.is_none()` gate at ~:1634 must now be TRUE for the END so it advances the online seed + stamps sfn).
- `backlog_drain.rs` — `ensure_and_drain_session_end` / `mint_session_end_prepared` (~:2597): the END-mint no longer acquires a pool code; the END drains as an online doc (Sent→Kvt1→Kvt2→Ack with a server_fiscal_no).

## 5. RED-first pins (write failing first)

1. The drain-minted END emits **bare `<MAC>`** (no `ID=`) — not `<MAC ID>`.
2. The END's wire envelope has **empty `id_offline`**; typCheck=ServiceChk(3).
3. The END does **NOT consume a pool code** (pool count unchanged by the END mint).
4. The END is an **online issuance**: on Sent it advances the online seed + gets a `server_fiscal_no`.
5. **D2:** pre-SENT reject of the END → `Rejected`, seed not advanced; post-SENT/ambiguous → `RequiresManualReconciliation`, seed not rolled back.
6. Full drain order/shape: `[BEGIN(offline <MAC ID>), SHIFT_OPEN(offline), SELL(offline), END(online bare <MAC>)]`; session `Draining→Closed` on END ACK.
7. **🦷 TEETH:** revert the END to offline-shaped (`<MAC ID>` + code) → pin #1/#2 REDs.

## 6. Blast radius — update, don't delete

- **B10 unit tests** (`tests/b10_offline_session_handshake.rs`): several asserted the END was offline-shaped (`<MAC ID>` + consumed code, e.g. `b10_boundary_docs_consume_exactly_two_codes`, the teeth `b10_end_signed_offline_shaped_with_mac_id_under_going_online`). These now flip to **online-shaped** — UPDATE them to the new truth (the END is bare-`<MAC>`, no code); the old `*_offline_shaped_*` teeth becomes an online-shaped teeth. Re-derive each affected assertion; do not just delete.
- **Fuzzer RefModel** (`tests/invariant_fuzzer/model.rs`): the approach-d two-doc model predicted the END as an OFFLINE doc (consumes a code, offline ledger). It now predicts the END as an **ONLINE issuance** (no code consumed, advances the online seed, gets a server_fiscal_no). Re-reconcile `drain_backlog`/END prediction + the teeth canary `teeth_b10_reverted_begin_chain_reddens_ledger_delta` (the END is now online, so its ledger-delta shape changes). Keep teeth — model predicts independently.
- **live smoke 9** (`live_dps_extended_smoke.rs`): with the fix, the drain's own END will be online-shaped; the manual recovery test can stay as a documented recovery tool. Re-run (operator) proves `[BEGIN,SHIFT_OPEN,SELL,END]` all → ACK + clean `Draining→Closed`.

## 7. Verification / acceptance

- Full gate green: `cargo nextest run -p prro --features test-support` (baseline 1751-pass — the END-shape change will flip the B10 + fuzzer END tests; all must be re-established green) + fmt + clippy, read from OUTPUT.
- **D2 re-review** (architect, 2 lenses) — the END-as-online-issuance is the focus: seed-advance-at-SEND, pre/post-SENT split, no double-issue, chain off the content tip.
- LIVE (operator) smoke 9 → `[BEGIN,SHIFT_OPEN,SELL,END]` all ACK, `online→true`, no dangling session. This is the merge gate.

## 8. Delivery (7-item format) + note

State how each D2 point (§3) holds, the blast-radius table (each flipped test → new assertion), and the teeth proof. This CLOSES the offline line → B10 merge.
