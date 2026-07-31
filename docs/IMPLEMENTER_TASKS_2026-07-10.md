# Implementer task package — 2026-07-10

**Author:** architect · Companion rulings: `docs/RULINGS_2026-07-10_SHIFT_T112_AUTOZ.md` (authoritative for all semantics below). Discipline for EVERY task: STRICT RED-first TDD (pins fail first), minimal diff, worktree isolation, hot-zone rules (`CLAUDE.md`), 7-item delivery, **mandatory «Fuzzer impact» section** in the delivery (rule 2026-07-10: every feature states which fuzzer wave/model/oracle must track it — in the same increment or as a named issue). The required CI gate (#253) runs the full suite incl. the invariant fuzzer on every rust-touching PR.

Order: **T1 → T2 → T3** (T2 is a hard prerequisite of T3's unconditional auto-Z). The RAGE fuzzer waves (`docs/FUZZER_TIER2_RAGE_DOSSIER.md`, W1→W2 first) run in PARALLEL by the companion LLM.

---

## T1 — Bounded superseded-hold for shift-lifecycle docs → RMR (RULING 1 / PRRO_GATE-eid)

**Hot zones:** `services/reconciliation/online_convergence.rs` (the `SupersededHeld` arm at ~:267), audit vocabulary, possibly a small durable counter surface.

**Change.** In the online convergence tick, when `confirm_drain_doc` yields `SupersededHeld` for a doc whose `doc_type ∈ {SHIFT_OPEN, Z_REPORT, SHIFT_CLOSE}`: increment a DURABLE per-doc superseded-held tick counter; when it reaches `SUPERSEDED_SHIFT_HOLD_TICKS` (tunable const, default 5) → escalate the FN to `RequiresManualReconciliation` via the existing escalate CAS (doc state untouched), emitting `CONVERGE_SUPERSEDED_SHIFT_BOUND_ESCALATE_MANUAL` with `{document_id, doc_type, held_ticks}`. Receipt docs (SELL/RETURN): behavior byte-unchanged (unbounded benign hold, AUD-L5-1). A successful confirm on a later tick resets/obsoletes the counter (escalation must not fire after recovery).

**Durability:** the counter must survive restart (persist it — e.g. an audit-derived count or a small column/kv; choose the narrowest seam, justify it; NO schema churn without migration reasoning). A crash at tick N and reboot must not reset the bound.

**RED-first pins:** (1) shift-doc superseded ×N → FN → RMR + the dedicated audit (today: RED — it holds forever); (2) shift-doc superseded ×(N−1) then confirm-success → NO escalation, shift converges; (3) SELL superseded ×(N+5) → still benign hold (no escalation) — the receipt arm is untouched; (4) crash between held-ticks → counter survives, bound still fires at N total; (5) 🦷 teeth: revert the bound (restore unbounded hold) → pin 1 REDs.

**Fuzzer impact (mandatory):** unblocks RAGE W1-7 — after T1 merges, the companion enables `Superseded` scripts on shift-class ops; the model predicts held≤N→RMR; the known-red tooth `teeth_d5_shift_doc_superseded_known_red` converts to a normal differential + teeth. Coordinate: T1's PR text must state the model contract (N, reset rule) verbatim.

---

## T2 — Offline code close-reserve (RULING 3.5 prerequisite; operator «пиздец важно» 2026-07-08)

**Hot zones:** `stage_sign`/`acquire_code_tx` (pool acquisition), offline session/shift state read, `backlog_drain` (END is online — consumes zero).

**Change.** Enforce a DYNAMIC legal close-reserve on the offline pool (per RAGE W3-3 — do NOT freeze a magic `≤2`):
`required_codes_to_close = (session BEGIN missing ? 1 : 0) + (offline Z still needed for the open shift ? 1 : 0)` — the DocType=10 END is online-shaped and needs none.
An **ordinary** offline op (SELL/RETURN) is refused fail-closed (pre-mint, no row — the 503-family refusal) when granting its code would leave `free_codes < required_codes_to_close`. **Close-path ops (offline Z_REPORT, the lazy BEGIN mint, session close) always may draw from the reserve.** Keep the operational refill watermark (`min_offline_codes`) SEPARATE from this legal reserve — replenish thresholds do not substitute for the gate.

**RED-first pins:** (1) pool=2, no BEGIN yet, open shift → offline SELL refused (would leave 1 < required 2), NO row/lnd/code (today: RED — it consumes); (2) same state → offline Z ALLOWED (draws reserve), BEGIN+Z mint, shift closes locally; (3) pool=1, BEGIN exists → SELL refused, Z allowed; (4) reserve never blocks ONLINE ops; (5) refusal is the row-less 503 family (audit-only), consistent with the shift-class lane; (6) 🦷 teeth: revert the gate → pin 1 REDs. Invariant statement: **«a shift is NEVER wedged un-closable for lack of a code»** — cite it in the PR.

**Fuzzer impact (mandatory):** RAGE W3-3 adds the standing oracle `free_codes >= required_codes_to_close` after every successful ordinary offline op — land the oracle in the same increment or file the issue; the model's pool-exhaustion arms (`apply_sell`/shift/Z) gain the reserve refusal branch.

---

## T3 — Offline/shift time budgets + UNCONDITIONAL auto-Z (RULING 3; operator ТЗ 2026-07-10)

**Prereq:** T2 merged. **Hot zones:** shift/offline state, write_path admission gates, ops-loop (auto-Z trigger), config, possibly migration (durable accumulator / opened_at exposure).

**Change.**
1. **Three document-derived budgets** (RULING 3.1): 168h/calendar-month cumulative offline; 36h continuous offline session; 24h shift duration. Derived from durable rows (SHIFT_OPEN business_ts; offline_sessions opened_at/closed_at; a monthly accumulator recomputed from sessions — prefer recompute-on-read over a mutable counter if cheap enough). All three exposed via metrics/audit, tracking ALWAYS on.
2. **Enforcement gates** (config-toggleable per budget, default ON): over-budget NEW fiscal ops are refused fail-closed pre-mint (row-less 503 family). The legal close path (Z, session END, drain) is NEVER blocked by enforcement — closing is always allowed.
3. **Unconditional auto-Z** (RULING 3.4): an ops-loop ticker watching the 24h shift budget makes a durable Z attempt at the boundary regardless of toggles: online → normal Z dispatch; offline → offline Z (reserve guaranteed by T2); failure → the existing Z failure routes (retry/RMR), never silent continuation. `shift_autoclose_enabled` deprecated: parsed-but-ignored + one-time deprecation audit. A short superseding note is added to the May spec file header (`2026-05-30-offline-shift-limits-spec.md`) pointing to RULING 3.
4. **Clock seam:** all three budgets + the ticker read ONE injectable clock (prod = system UTC; tests inject). No wall-clock reads scattered in hot paths. Backwards input clamps (no negative budgets, no fail-open).

**RED-first pins:** (1) shift at 24h+ε → durable Z attempt exists (state Z-issued or an escalated failure), toggle OFF included — the core unconditional pin; (2) offline session at 36h+ε with enforcement ON → new SELL refused, Z/close allowed; (3) month accumulator at 168h−ε then +2h session → over-budget refusal at the boundary, month rollover resets 168 but NOT a running 36h session; (4) enforcement OFF → no refusals, but tracking metrics still move AND pin 1 still holds; (5) budgets derived from documents survive reboot (recompute equals pre-reboot values); (6) backwards clock → no negative/fail-open; (7) 🦷 teeth: revert the unconditional ticker → pin 1 REDs.

**Fuzzer impact (mandatory):** this increment + RULING 3 note satisfy the RAGE W5 normative gate. W5 then lands `Op::AdvanceClock` against the SAME clock seam — the seam's test-injectability is part of THIS task's acceptance. Fence: until W5 lands, the three boundaries get directed pins here; the generative time-axis is the companion's wave.

---

## Review protocol (all tasks)

Architect reviews each PR with two lenses (invariant + break-it) before merge; live smoke where the task touches wire behavior (T3's auto-Z online leg can be smoke-tested on the test cabinet). Deliveries without the «Fuzzer impact» section are returned unreviewed.
