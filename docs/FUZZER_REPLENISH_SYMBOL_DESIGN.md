# Fuzzer alphabet: generative `Replenish` (T=112) — design

**Why now.** `bd PRRO_GATE-hpc` (PR #343, merged `05a9c694`) added real production seed semantics for a
standalone T=112 offline-code replenish — a durable `chain_seed_transitions` witness, a new ordering
rule folded into `active_chain_tip_unsigned_xml_sha256`, and a re-anchor in the `invariant_scan`
MAC-walk. All of it has **directed coverage only**: the fuzzer alphabet has **no `Replenish` symbol**,
so none of it is exercised generatively (composed with crashes, drains, operator completions, reports).
This is the widest remaining alphabet gap and it was widened by our own change.

Evidence that alphabet beats scale here: the first nightly after the CS-3 track merged (which enabled
the generative `NotAcceptedOffline` op) found `PRRO_GATE-x5o` **immediately** at the same N that had
been green for weeks. A new symbol found it — not more cases.

---

## 1. Scope — DECIDED cases only

`RULING 2` (`docs/RULINGS_2026-07-10_SHIFT_T112_AUTOZ.md`, §4) is explicit: until a live capture of a
real ambiguous/timeout T=112 lands (that is `bd PRRO_GATE-2ds`, blocked on the operator), **the
generator does NOT emit ambiguous-T112**. So this slice emits exactly the two DECIDED outcomes:

| Leaf | Wire | Prod contract | In this slice |
|---|---|---|---|
| success | `ask_offline_codes` → `OfflineCodesResponse` with N codes | pool insert (`INSERT OR IGNORE`) + seed → `Hs` + witness, all in ONE `with_immediate` | **YES** |
| server reject | `DpsError` (server-side reject) | NO persist, NO seed advance, typed error | **YES** |
| ambiguous / transport timeout | transport error | no retry, fresh-request recovery (RULING 2 §1-2) | **NO — blocked on 2ds** |

Keeping the ambiguous leaf out is not a shortcut: emitting it would pin a contract that is explicitly
`known-red until captured evidence`.

---

## 2. Feasibility blocker — `FuzzCtx` has no `App`

`OfflineCodeReplenishService::new(app: App, dps, sign_ctx)` needs an `App` (it provides the per-FN
write gate AND the pool — `offline_code_replenish.rs:157-162`). `App` exposes only
`App::boot(config)`; `Inner` is private, so there is no way to wrap the fuzzer's existing pools.

`FuzzCtx` today builds its own pools (`fresh_pool()` / `fresh_secure_pool()`) and its own
`gate: Arc<Mutex<()>>`, in four fixture constructors (`new_online_open_shift`,
`new_online_closed_shift`, `new_offline_open_shift(codes)`, `new_offline_closed_shift(codes)`).

**Rejected alternative — reproduce the replenish's DB effects inside `interp`.** That would make the
interpreter model production instead of driving it: the oracle would then agree with itself by
construction (vacuous). The whole point of the fuzzer is that the interpreter drives REAL seams.

**Chosen — `FuzzCtx` owns a real `App`, and its pool IS `app.db()`.** Boot the App first from a temp
config, then take `app.db().clone()` as the fixture's `pool` so every existing op keeps working
against the same database. `boot_app()` in `tests/hpc_t112_nc03.rs` already demonstrates the recipe
(temp dir + `AppConfig` + `App::boot`).

Costs and risks, stated honestly:
- **Per-case cost.** `App::boot` runs migrations + opens the secure pool — roughly what the fixture
  already does, plus config wiring. Measured indirectly: the 8 hpc tests that each boot an App run in
  well under a second total. At 4096 cases this is minutes, not hours — but it MUST be measured on the
  capstone before/after, and the number recorded here.
- **Two gates.** The fixture's `gate` (used by `inline::run`) and `app.acquire_fn_gate` would be
  different mutexes. Ops are driven sequentially by the harness, so this does not break the fuzzer, but
  it is a fidelity wrinkle: the single-writer invariant is then enforced by two independent locks.
  Prefer routing the fixture's inline calls through the App's gate if the types allow; if not, document
  the split explicitly rather than leaving it implicit.

---

## 3. Model prediction

A replenish is unlike every existing op: it **mints no document**, allocates **no lnd**, and yet
**advances the seed** and **grows the code pool**.

- `codes_issued += n` (the pool insert is `INSERT OR IGNORE`, so a duplicate code is deduped — the
  model must mirror the dedup, not assume `+n` blindly).
- `seed` → a NON-DOC value. The model cannot compute `sha256(request_xml)` (it does not build XML), so
  it records the advance **structurally** (the existing differential is structural: *advanced iff the
  model advanced*), exactly as the `NotAcceptedOffline` arm records the rewind structurally.
- `next_lnd` UNCHANGED — this is the load-bearing difference from every issuance op, and it is what
  makes the hpc `lnd_at_write` ordering frame meaningful.
- Server reject → **nothing** changes (no codes, no seed, no witness).

---

## 4. Oracle work (the real content of this slice)

1. **A seed advance with no document.** `run_harness`'s structural seed check and the B3 "a no-op op
   allocates no lnd / does not advance the seed" invariants assume seed movement implies issuance. A
   `Replenish` breaks that assumption legitimately. Either a new `ExpectedOutcome` variant or an
   explicit carve-out keyed on the op — a carve-out must be narrow and asserted (the op DID advance the
   seed AND allocate no lnd), never a blanket exemption.
2. **`invariant_scan` must stay clean** after a replenish, and after `replenish → offline SELL`
   (the doc chains onto `Hs`). This is exactly what the hpc fix implements; the generative symbol is
   what proves it under composition. A regression here is a REAL finding.
3. **NC-03 composition.** `Replenish → Crash → Reboot` must recover `Hs` (the hpc witness). Directed
   tests cover the recovery; the generator composes it with everything else.
4. **Code accounting.** The existing offline-code oracles (`codes_issued` / `codes_consumed`,
   `OfflineFiscalNoUnbacked`) must stay consistent once codes can arrive mid-sequence rather than only
   at fixture time.

---

## 5. Tests / teeth

- Directed generative tooth: `[OfflineSell, Replenish, OfflineSell]` through `run_harness` — the second
  sell chains onto `Hs`; scan clean; codes accounted.
- Directed: `[Replenish(reject)]` — nothing mutated (no codes, no seed advance, no witness).
- Directed: `[Replenish, Crash(..), Reboot]` — NC-03 recovers `Hs` (composes the hpc witness).
- Canaries: neutralise the model's `codes_issued` update → RED; neutralise the structural seed advance
  → RED; make the reject leaf mutate → RED.
- Capstones at the default N, then one 4096 run before enabling in nightly.

---

## 6. Sequencing

1. Land `x5o` first (the nightly is RED until it merges; adding a symbol on a red nightly makes
   attribution impossible).
2. Fixture refactor (`FuzzCtx` owns an `App`) as its own commit — behaviour-neutral, full suite green.
3. `Op::Replenish` + interp + model + generator + teeth.
4. Measure the capstone wall-clock before/after and record it here.
5. Only then consider raising nightly N (and prefer 2 × 4096 shards over one 8192 job).

---

## 7. As-built (2026-07-31, branch `feat/fuzzer-replenish-symbol`)

**§2 fixture.** Taken as designed: `FuzzCtx` owns a real `prro::App`; `pool` / `pool_secure` are clones
of `app.db()` / `app.db_secure()`, so every pre-existing op runs against the SAME database. `App::boot`
spawns no background tasks (the loops live in `runtime::supervisor::run`, started only by `serve`), so
determinism is unaffected.

**§2 "two gates" — NOT unified, documented instead.** The public API exposes only
`App::acquire_fn_gate() -> OwnedMutexGuard<()>` (`app.rs:402`), never the `Arc<Mutex<_>>`, and `Inner`
is private — so `inline::run`'s gate cannot be routed through the App's. This is sound because the
harness drives ops strictly sequentially (one op fully completes before the next begins), so the two
locks are never contended. The honest consequence, recorded at the `gate` field in `interp.rs`: this
harness does **not** exercise invariant #2 (one FN = one writer) as a *concurrency* property — that
remains the job of the dedicated concurrency tests.

**§3 model.** As designed, plus ONE contract the design did not anticipate — see below.

**§4 oracle.** Implemented as a narrow, asserted `run_harness` carve-out rather than a new global
outcome shape: per Replenish op it asserts the seed moved **iff** granted, `next_lnd` unchanged, no doc
minted, no code consumed, pool delta `== inserted`, and `inserted + deduped >= 1`. The
`inserted`/`deduped` split is what keeps the `INSERT OR IGNORE` dedup honest (§3) without asserting a
blind `+n`.

### The finding: the S7-2 fence refuses a replenish under an active reservation

The generator composed `[… → held reservation → Replenish]` and production refused
(`fn_fence_active_tx`: *FN … has an active delivery reservation*) where the model predicted `granted`.
**Adjudicated prod = correct** — the S7-2 fence exists precisely so a seed-moving operation cannot
interleave with an unresolved delivery. Taught to the model as an explicit precondition
(`held_reservation.is_some() → granted: false`); production was NOT weakened.

Known-narrow gap, documented at the check rather than papered over: a CALL_STARTED crash can leave the
fence raised with no model-visible hold, so a future sequence could see prod refuse where the model
still says granted. That would be a REAL finding about fence-residue recovery, not a model bug — it is
left to fail loudly.

### Capstone measurement (§1 point 4)

MEASUREMENT_PLACEHOLDER
