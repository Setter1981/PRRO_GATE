# Architect rulings — 2026-07-10 (unblocks RAGE waves W1/W3/W5 + prod tasks)

**Author:** architect · Grounded in: `online_convergence.rs:267-277` (AUD-L5-1), `er_redrive_policy` / `last_chk_probe`, M3b §16.7, WebCheck `SubmitPtr.cs` / `Offlin.cs`, operator rulings 2026-07-08/10, live campaign evidence.

---

## RULING 1 — PRRO_GATE-eid: online `Superseded` on a SHIFT-LIFECYCLE doc gets a BOUNDED hold → RMR

**Problem.** An online SHIFT_OPEN / Z_REPORT whose KVT1 confirm returns `SupersededHeld` rests in a benign hold with **no bound** (`online_convergence.rs:267-277`: hold + Warning + dashboard counter, no escalation), leaving the shift stuck in `Opening`/`Closing`. The offline drain escalates the SAME outcome to RMR (ruling B). The fuzzer surfaced this (model expected a terminal; production holds forever).

**Ruling.**
1. **Shift-lifecycle docs (SHIFT_OPEN / Z_REPORT / SHIFT_CLOSE) in `SupersededHeld` on the online convergence tick get a BOUNDED hold: after `N` consecutive superseded-held ticks for the same doc → escalate the FN to `RequiresManualReconciliation`** (reuse the existing escalate CAS; the doc rests where it is — no state invention). Rationale: a stuck `Opening`/`Closing` transient wedges the shift (can neither open nor close, compounds the shift time-limit), and RMR is the designed operator surface for "cannot drive the shift to a terminal" (M3b §16.7 family 2 spirit — edges 4/12). The genuine resolution (the superseding doc settles, KVT confirm succeeds) normally lands within 1-2 ticks, so a bound does not fire on healthy traffic.
2. **Receipt docs (SELL/RETURN) in `SupersededHeld` stay benign-hold unbounded** — AUD-L5-1 stands for them verbatim (a held receipt does not wedge the shift; the counter + Warning remain the surface).
3. **Bound value:** a dedicated tunable `SUPERSEDED_SHIFT_HOLD_TICKS` (default **5**; deliberately > `HOLD_INDETERMINATE_CRITICAL_TICKS = 3` since superseded has a benign self-resolution path). Persisted tick-count derivation must be durable (crash-safe), not in-memory.
4. **Audit:** the escalation emits a dedicated event (e.g. `CONVERGE_SUPERSEDED_SHIFT_BOUND_ESCALATE_MANUAL`) with the tick count — operators must be able to distinguish this from chain-seed / indeterminate escalations.

**Consequences:** prod task T1 below; the fuzzer (W1-7) may enable `Superseded` on shift-class ops ONLY AFTER T1 lands — model: online shift-doc superseded → held ≤ N ticks → RMR; the known-red tooth then converts into a normal differential + teeth.

---

## RULING 2 — PRRO_GATE-2ds: T=112 ambiguous outcome = FRESH-REQUEST recovery; no byte-identical resend, no WebCheck retry-loop

**Problem.** WebCheck retries T=112 via `SubmitCheck`'s in-line `All.Retries=18` loop; our `OfflineCodeReplenishService` deliberately makes one call with no retry, because T=112 is not proven idempotent. Real prod-vs-reference discrepancy; the lost-response branch (DPS may have issued codes + advanced its tip while we saw nothing) is unmodeled.

**Ruling.**
1. **No byte-identical resend and no in-line retry loop.** Same doctrine as the drain `-8` decision: WebCheck's `Thread.Sleep×18` is a crude client; we do not copy mechanisms, only calibrate tolerances. A byte-identical T=112 resend risks double-allocation on an unproven-idempotent endpoint.
2. **Recovery = the NEXT replenish attempt is a FRESH request (new DI, new TS) on the service's normal cadence.** Grounds: live campaign evidence shows DPS re-issues allocated-but-unconsumed codes on subsequent T=112 calls (the same opaque codes returned across our runs), so a lost response does not strand the codes — a fresh request re-obtains them. The pool inserts are dedup-by-value, so re-issued codes are idempotent locally.
3. **The lost-response chain-tip hazard is handled by the existing settle discipline:** after an ambiguous T=112 the local tip may lag DPS's. The next fiscal send discovers this via the normal chain rules; no special-case tip adoption. The fuzzer (W3-1) models local vs remote tips separately and pins exactly this: an ambiguous T=112 followed by a fiscal doc must recover (fresh tip discovery), never silently fork.
4. **Known-red stands until captured evidence:** the live campaign should capture one real ambiguous/timeout T=112 on the test cabinet (kill the connection mid-call) and record what a subsequent fresh T=112 returns. If evidence contradicts (2) — e.g. DPS double-allocates windows — this ruling reopens. Until then the generator does NOT emit ambiguous-T112; the model/impl contract above is pinned by directed tests when W3 lands.

**Consequences:** current prod behavior (no in-line retry) is CONFIRMED correct — no prod change; W3-1/W3-2 get their contract; a live-campaign capture item is added.

---

## RULING 3 — W5 policy note: three budgets; tracking always-on; enforcement toggleable; auto-Z UNCONDITIONAL (supersedes `shift_autoclose_enabled`)

**Problem.** The May spec (`2026-05-30-offline-shift-limits-spec.md`) has a per-FN `shift_autoclose_enabled` toggle; the operator ruling 2026-07-10 says the auto-Z at the shift limit is unconditional. The RAGE W5 gate requires this conflict resolved before code.

**Ruling (supersedes the May spec where they conflict):**
1. **Three document-derived budgets:** 168h cumulative offline per calendar month · 36h continuous offline session · 24h shift duration. All three are **derived from durable documents** (SHIFT_OPEN business_ts, offline session opened_at/closed_at, calendar-month accumulator) — never from in-memory wall-clock state; they survive restart/crash by construction.
2. **Tracking is ALWAYS ON** — not toggleable. The budgets are computed and exposed (metrics/audit) unconditionally.
3. **Enforcement is config-toggleable per budget** (refusing NEW fiscal ops over-budget, fail-closed). Default ON. Turning enforcement off is an operator decision that never disables tracking or the auto-Z below.
4. **The shift-limit auto-Z is UNCONDITIONAL.** When the 24h shift budget is reached, the gateway makes a durable Z attempt regardless of any toggle: a shift never crosses the limit without a durable Z attempt/outcome (success, or an escalated failure — RMR — but never silent continuation). `shift_autoclose_enabled` is deprecated: existing config keys parse but are ignored with a deprecation audit, and the spec note supersedes §-references to the toggle.
5. **Offline dependency:** an unconditional auto-Z in OFFLINE mode consumes a pool code — therefore the **close-reserve (T2 below) is a hard prerequisite**: the reserve invariant guarantees the Z (and session END path) is never blocked by an empty pool. Order: T2 lands before (or with) T3.
6. **Clock discipline:** one injected control-flow clock (W5-1) drives admission/accumulation/auto-Z/cert-gate; document business timestamps remain the durable inputs. Backwards clock input must not produce negative budgets or fail-open behavior.

---

## Task order (see IMPLEMENTER_TASKS_2026-07-10.md)

T1 superseded-shift bound (small, unblocks W1-7) → T2 offline code close-reserve (small-medium, «пиздец важно», prerequisite of T3) → T3 time limits + unconditional auto-Z (medium). RAGE W1/W2 (fuzzer, companion LLM) runs in parallel — the required CI gate (#253) is already live.
