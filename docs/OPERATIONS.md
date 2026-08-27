> **⚠️ LEGACY — RETIRED PYTHON SCAFFOLD. DO NOT USE.**
> This document describes the pre-Rust Python runtime, which is not built, not tested and not
> deployed. The live gateway is the Rust workspace under `rust/` — run it with
> `cargo run -p prro -- serve --config <path>`; commands, gates and architecture are documented
> in `CLAUDE.md`, `docs/CONSOLIDATION_SPRINT_ROADMAP.md` and `docs/LEGAL_INVARIANTS.md`.
> Kept only as history (external review 2026-08-27).

# OPERATIONS

## Health endpoints

- `/health/live`
- `/health/ready`
- `/health/startup`

Each endpoint includes the current runtime `phase`.

## Startup phases

- `CREATED`
- `PHASE1_STARTING`
- `PHASE1_COMPLETE`
- `PHASE2_RECONCILING`
- `RUNNING`

## Expected production mode

`runtime.process_immediately = false`

Ingress should durable-accept commands and let the worker process them out-of-band.
`process_immediately = true` remains a dev/test convenience only.
