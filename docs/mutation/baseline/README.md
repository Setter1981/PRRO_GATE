# Mutation baseline database

Machine-consumed by `scripts/mutation/run.sh` (see `../README.md`).

- **`survivors.txt`** — one mutant per line, the mutants NO test kills. This is
  the actionable list: each line is a place the code could be wrong silently.
  Consumed with `comm` to compute NEW / CLOSED survivors on each run. Seeded
  empty; filled + refreshed by a `SCOPE=full` run (`run.sh full`, on a server).
- **`outcomes.json.gz`** — full cargo-mutants record (every mutant + verdict), gzipped.
- **`mutants.json.gz`** — the mutant catalog, gzipped.

Do **not** hand-edit `survivors.txt`. To shrink it, add a test that kills the
survivor, then a full run drops it (reported as 🟢 CLOSED). To grow it knowingly
(a survivor we accept for now), a full run adds it — review the diff before
committing.

## First full baseline — 2026-07-14

Whole-workspace `SCOPE=full` run on `origin/main` @ `3e2088b` (Hetzner CCX63,
`CARGO_INCREMENTAL=0 -C debuginfo=0 -j10`, ~8h47m).

| metric | value |
|---|---|
| total mutants | 10068 |
| **caught** | **2734** |
| **survivors (missed)** | **157** |
| unviable | 7166 (71% — `Default::default()`-family) |
| timeout | 11 |
| **kill rate** | **94.6 %** of viable (2891) |

`survivors.txt` is this run's raw 157. Fiscal-relevant survivors were triaged
per-function (real-vs-equivalent); see `docs/MUTATION_TEETH_BACKLOG.md`. Known
deltas the NEXT full run will apply:

- 🟢 CLOSED by teeth PRs: cash-EPZ tx-aggregate (#275); force-seam evidence +
  actor + cp1251 ×17 (#276); auto-Z idempotency key (#278).
- ⚪ RETIRED via `.cargo/mutants.toml exclude_re` (#277): `max_submitted_lnd`,
  `last_ack_unsigned_xml_sha256`, `current_open_or_draining_session_id_tx` (11
  mutants) — dead/retained, not oracle gaps.

The scoped `error_routing.rs` pilot (2026-07-13) recorded **0 survivors**.
