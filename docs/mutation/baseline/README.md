# Mutation baseline database

Machine-consumed by `scripts/mutation/run.sh` (see `../README.md`).

- **`survivors.txt`** — one mutant per line, the mutants NO test kills. This is
  the actionable list: each line is a place the code could be wrong silently.
  Consumed with `comm` to compute NEW / CLOSED survivors on each run. Seeded
  empty; filled + refreshed by a `SCOPE=full` run (`run.sh full`, on a server).
- **`outcomes.json`** — full cargo-mutants record (every mutant + verdict).
- **`mutants.json`** — the mutant catalog.

Do **not** hand-edit `survivors.txt`. To shrink it, add a test that kills the
survivor, then a full run drops it (reported as 🟢 CLOSED). To grow it knowingly
(a survivor we accept for now), a full run adds it — review the diff before
committing.

Seeded: `outcomes.json` / `mutants.json` are absent until the first full server
run. The scoped `error_routing.rs` pilot (2026-07-13) recorded **0 survivors**.
