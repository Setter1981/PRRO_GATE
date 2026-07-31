# Architect rulings — 2026-07-10 (unblocks RAGE waves W1/W3/W5 + prod tasks)

**Author:** architect · Grounded in: `online_convergence.rs:267-277` (AUD-L5-1), `er_redrive_policy` / `last_chk_probe`, M3b §16.7, WebCheck `SubmitPtr.cs` / `Offlin.cs`, operator rulings 2026-07-08/10, live campaign evidence.

---

## RULING 1 — PRRO_GATE-eid: online `Superseded` on a SHIFT-LIFECYCLE doc gets a BOUNDED hold → RMR

**Problem.** An online SHIFT_OPEN / Z_REPORT whose KVT1 confirm returns `SupersededHeld` rests in a benign hold with **no bound** (`online_convergence.rs:267-277`: hold + Warning + dashboard counter, no escalation), leaving the shift stuck in `Opening`/`Closing`. The offline drain escalates the SAME outcome to RMR (ruling B). The fuzzer surfaced this (model expected a terminal; production holds forever).

**Ruling.**
1. **Shift-lifecycle docs (SHIFT_OPEN / Z_REPORT / SHIFT_CLOSE) in `SupersededHeld` on the online convergence tick get a BOUNDED hold: after `N` TOTAL superseded-held ticks for the same doc (cumulative; the doc leaving the pending cohort via confirm-success is the ONLY reset) → escalate the FN to `RequiresManualReconciliation`** (reuse the existing escalate CAS; the doc rests where it is — no state invention). Rationale: a stuck `Opening`/`Closing` transient wedges the shift (can neither open nor close, compounds the shift time-limit), and RMR is the designed operator surface for "cannot drive the shift to a terminal" (M3b §16.7 family 2 spirit — edges 4/12). The genuine resolution (the superseding doc settles, KVT confirm succeeds) normally lands within 1-2 ticks, so a bound does not fire on healthy traffic.
2. **Receipt docs (SELL/RETURN) in `SupersededHeld` stay benign-hold unbounded** — AUD-L5-1 stands for them verbatim (a held receipt does not wedge the shift; the counter + Warning remain the surface).
3. **Bound value:** a dedicated tunable `SUPERSEDED_SHIFT_HOLD_TICKS` (default **5**; deliberately > `HOLD_INDETERMINATE_CRITICAL_TICKS = 3` since superseded has a benign self-resolution path). Persisted tick-count derivation must be durable (crash-safe), not in-memory.
4. **Audit:** the escalation emits a dedicated event (e.g. `CONVERGE_SUPERSEDED_SHIFT_BOUND_ESCALATE_MANUAL`) with the tick count — operators must be able to distinguish this from chain-seed / indeterminate escalations.

**Amendment (2026-07-10, post-review adjudication):** the original wording said "N *consecutive*"; the landed T1 implementation counts **cumulative** superseded-held ticks (audit-derived lifetime count, reset ONLY on cohort exit / ACK — interleaved `HoldFnDrain` ticks do NOT reset). Adjudicated ACCEPTED: both hold outcomes are non-progress for a shift-lifecycle doc, so cumulative counting escalates a genuinely-wedged shift slightly earlier in mixed-hold scenarios — conservative in the right direction; no false escalation on healthy traffic (recovery before N and receipts are pinned). This paragraph supersedes "consecutive" wherever it appears.

**Consequences:** prod task T1 below; the fuzzer (W1-7) may enable `Superseded` on shift-class ops ONLY AFTER T1 lands — model contract (per the amendment, NOT the implementer's original delivery note): online shift-doc superseded-held ticks accumulate CUMULATIVELY per doc; at 5 total → FN → RMR; the count is reset ONLY by the doc converging (ACK / cohort exit), NOT by interleaved non-superseded holds; receipts never bound. The known-red tooth then converts into a normal differential + teeth.

---

## RULING 2 — PRRO_GATE-2ds: T=112 ambiguous outcome = FRESH-REQUEST recovery; no byte-identical resend, no WebCheck retry-loop

**Problem.** WebCheck retries T=112 via `SubmitCheck`'s in-line `All.Retries=18` loop; our `OfflineCodeReplenishService` deliberately makes one call with no retry, because T=112 is not proven idempotent. Real prod-vs-reference discrepancy; the lost-response branch (DPS may have issued codes + advanced its tip while we saw nothing) is unmodeled.

**Ruling.**
1. **No byte-identical resend and no in-line retry loop.** Same doctrine as the drain `-8` decision: WebCheck's `Thread.Sleep×18` is a crude client; we do not copy mechanisms, only calibrate tolerances. A byte-identical T=112 resend risks double-allocation on an unproven-idempotent endpoint.
2. **Recovery = the NEXT replenish attempt is a FRESH request (new DI, new TS) on the service's normal cadence.** The pool inserts are dedup-by-value, so re-issued codes are idempotent locally.

   > **CORRECTION 2026-07-31 — this clause overstated its evidence.** It originally continued: *"Grounds: live campaign evidence shows DPS re-issues allocated-but-unconsumed codes on subsequent T=112 calls (the same opaque codes returned across our runs), so a lost response does not strand the codes — a fresh request re-obtains them."* The observation is real; the conclusion does not follow. **No run in that campaign ever lost a response mid-call**, so the ambiguous case was never exercised. Two hypotheses remain live and mutually exclusive:
   > - **(H1)** each T=112 allocates a **fresh** range → a lost response **leaks** it server-side. This is what `offline_code_replenish.rs` asserted, in its own comment, also as fact.
   > - **(H2)** DPS **re-issues allocated-but-unconsumed** codes → the ambiguous case is free.
   >
   > Two places in the repo stated opposite things, both as settled fact. Neither is proven. It matters: under H1 a flapping link burns a range per break, eating the offline code-reserve floor (bd `PRRO_GATE-255`) and the monthly allocation; under H2 it costs nothing. **Only the §4 capture settles it.** The ruling's *operational* conclusion (fresh request; no byte-identical resend) holds either way — it is the *cost* claim that was unfounded.
   >
   > **SETTLED 2026-07-31 by the §4 capture — H2.** After a T=112 that DPS demonstrably *processed* (it had begun replying when the connection was torn down), a fresh T=112 returned **the same unconsumed code** (`e_rmC3Y4pqY` in both phases). Nothing was stranded; nothing leaked. The clause's original conclusion was right — it now rests on the case it always claimed to cover, instead of on runs that never lost a response. H1 is not supported by observed behaviour. Limit: **N=1**; H2 is *supported*, not proven.

3. **The lost-response chain-tip hazard is handled by the existing settle discipline:** after an ambiguous T=112 the local tip may lag DPS's. The next fiscal send discovers this via the normal chain rules; no special-case tip adoption. The fuzzer (W3-1) models local vs remote tips separately and pins exactly this: an ambiguous T=112 followed by a fiscal doc must recover (fresh tip discovery), never silently fork.

   > ~~**Mechanism named, 2026-07-31.**~~ **RETRACTED THE SAME DAY — it was wrong, and it was wrong in the dangerous direction.** The retracted text claimed the stale `previous_hash` earns `-12` which routes to the *automatic bounded* `MacRecovery`, that `run_mac_recovery` re-drives attempt #2, and therefore that the MacReseed **guard-B** tip check is *never consulted* and an ambiguous T=112 *cannot deadlock*.
   >
   > **Every part of that is false post-CS-3 S7-1.** It was derived from the comment at `error_routing.rs:95` ("bounded ONE auto-recovery"), which is **stale**: S7-1 (R3) RETIRED the inline MAC-orchestrator loop along with its `mac_fx01..03` / `fx17` fixtures. The live contract is a `MacReseedPending` **HELD** — node → `STOP_MODE`, doc rests `SENDING` under a `PENDING_APPLY` reservation, **no second wire**. Pins: `write_path_stage4_send::minus_12_bad_hash_prev_records_held_stop_no_second_wire`, `record_outcome::rc05_bad_hash_prev_held_stop_mode`.
   >
   > So the `-12` path *is* the operator `MacReseed` path, and guard-B **is** consulted — the exact opposite of the retracted claim.
   >
   > **The deadlock the retraction was denying is real, and worse than a deadlock.** Proven by `hpc_t112_nc03::ambiguous_t112_leaves_the_minus_12_hold_unresolvable`:
   > ```
   > operator supplies Hs (DPS's actual tip): MacReseed seed does not match the expected chain tip
   > operator supplies Hp (the stale local tip): Ok(applied = true)
   > ```
   > guard-B accepts **only the value known to be wrong** and refuses the only value that would repair the chain. The one action the operator is permitted re-installs the stale tip, so the next send earns `-12` again and the node returns to `STOP_MODE` — a loop with no exit, in which every turn looks like the operator did something. Tracked as **`bd PRRO_GATE-3uo` (P1)**.
   >
   > The cause is isolated by the sibling test `guard_b_accepts_reseed_to_hs_rejects_hp`, which PASSES: there the T=112 *succeeded*, so it wrote a `chain_seed_transitions` witness and the active tip **is** `Hs`. The ambiguous arm writes no witness — and it discards the request XML whose `sha256` **is** `Hs`, so the value becomes locally unrepresentable forever.
   >
   > **Method note, recorded because the error is instructive:** the false claim was checked against a *comment* rather than against the *pins*. Two independent statements in this repo described the `-12` contract, one of them stale, and the stale one was the one that read like an authority. The pins were right and were three greps away.
   >
   > **Residual risk, NOT closed:** that self-heal parses a literal `"store "` tag out of a DPS message — a format inherited from the Python reference and **not yet observed from live DPS**. It fails loud (`HashNotExtractable`) on drift rather than corrupting, but a loud failure is still a stuck FN needing an operator. The §4 capture produces a real `-12` and should confirm the parse — so the capture settles **two** unknowns, not one.
   >
   > **DISCHARGED 2026-07-31 — with a caveat added the same day.** The capture forced a real `-12` and ran the production extractor against it: `regex_extract_store_hash` **parsed a live DPS message**, twice across two runs on different hashes. The Python-inherited format *is* what live DPS sends.
   >
   > **Caveat:** the parse is confirmed, but its ORCHESTRATOR is currently **unreachable from production**. Outside its own module, `run_mac_recovery` appears only in *comments* — there is no live call site after S7-1 retired the inline loop. What survives is the CLASSIFICATION (`RetryClass::MacRecovery`) and the constructed `MacRecoveryHint`, which carries the raw error text.
   >
   > **The text IS durably persisted — verified.** `stage_send.rs:1393` writes it into `AttemptCompletion.error_message` (`transport_trace.rs:117`), capped at 512 bytes (`CHECK`, `001_baseline.sql:633`, enforced upstream by `truncate_msg`). The live `-12` measures **160 bytes**, so the `store` field survives intact with 3× headroom, and `transport_trace::last_attempt_retry_class_for` (`:372`) already establishes the per-doc last-attempt read pattern.
   >
   > That closes the question that decided the size of `bd PRRO_GATE-3uo`: **`Hs` is not lost.** It is recorded by the very attempt that created the hold. The fix therefore does NOT need a new write on the ambiguous `replenish` arm — it needs guard-B to accept a seed **corroborated by the recorded `store` field** in addition to `active_chain_tip`. An unrelated operator seed still fails closed, so the #338 hardening is not weakened; it is arguably strengthened, since the seed must now match something the peer actually said rather than nothing at all.
   >
   > Live shape, for the record — note the **two** spaces after the code name:
   > ```
   > ERROR_BAD_HASH_PREV  store <64 hex> chk <64 hex>
   > ```
   > The capture also showed the tip **does** move on a T=112, so the post-kill fork is real and this recovery path is load-bearing rather than theoretical.

4. ~~**Known-red stands until captured evidence**~~ → **DISCHARGED 2026-07-31.** The capture ran on the test cabinet (FN `4000162280`): a T=112 was relayed to DPS and the connection torn down on DPS's first reply byte, producing the ambiguous shape (`transport error … peer closed connection without sending TLS close_notify … UnexpectedEof`) with the witness that DPS had *begun answering* — i.e. processed, not merely received. Evidence **confirms** (2) rather than contradicting it, so the ruling does not reopen.

   **Consequence: the generator exclusion is no longer justified by evidence.** Ambiguous-T112 was excluded from the fuzzer *only* because the contract had none. It now has some. Re-enabling the leaf is follow-up work (bd `PRRO_GATE-2ds`), not automatic — the model must first be taught what the captured behaviour implies, and N=1 is thin ground for a generative pin.

   Full log: bd `PRRO_GATE-2ds`. Harness: `rust/prro/tests/live_capture_ambiguous_t112.rs` (PR #351).

**Consequences:** current prod behavior (no in-line retry) is CONFIRMED correct — no prod change; W3-1/W3-2 get their contract; a live-campaign capture item is added.

> **§4 capture — PERFORMED 2026-07-31.** Design below, kept for the record; results in bd `PRRO_GATE-2ds` and in the §2/§3 notes above. Two things are worth carrying forward from doing it rather than planning it:
>
> - **The kill trigger matters more than the kill.** The first attempt armed the tear-down on "the client has gone quiet"; DPS answered inside that window, the RPC completed normally, and nothing ambiguous was produced. The correct trigger is the **first reply byte after the request was relayed** — it is simultaneously the strongest available witness that DPS *processed* the request and the exact instant to drop.
> - **The experiment cost nothing after all** — but only because H2 turned out to be true. That was not knowable in advance, which is why it was gated behind its own switch.
>
> Original design follows.
>
> What blocks this is NOT operator availability — the infrastructure already exists: `live_smoke_8_ask_offline_codes` sends a raw T=112 to live DPS and brackets it to reveal whether the MAC chain advanced; the kill-switch (`PRRO_LIVE_DPS=1`), the host allowlist and the test FN are all in place. Two things are missing:
>
> 1. **A deterministic mid-call connection kill.** The honest mechanism is a local TCP proxy that forwards the request to DPS and then drops the connection *before* the response — that guarantees the ambiguous shape (DPS received it; we did not hear back). A short client-side timeout is NOT equivalent: it cannot establish that the request actually reached DPS, which is the whole point.
> 2. **Acceptance that the experiment itself may burn a real code range** on the test FN — which is precisely the unknown being measured (H1 vs H2 above). Under H1 each attempt costs a range, so the run is not free and repeats are not free.
>
> The capture must record, in order: (a) the codes returned by a normal T=112; (b) the kill; (c) what a **fresh** T=112 returns — the *same* opaque codes (→H2) or a *new* range (→H1); (d) the MAC tip before/after; and (e) the raw text of the `-12` earned by the next fiscal send, to confirm the `"store "` hash extraction against a real DPS message.
>
> **Status:** designed, not run. Tracked as bd `PRRO_GATE-2ds`.

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
