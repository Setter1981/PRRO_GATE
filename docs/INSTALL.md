> **⚠️ LEGACY — RETIRED PYTHON SCAFFOLD. DO NOT USE.**
> This document describes the pre-Rust Python runtime, which is not built, not tested and not
> deployed. The live gateway is the Rust workspace under `rust/` — run it with
> `cargo run -p prro -- serve --config <path>`; commands, gates and architecture are documented
> in `CLAUDE.md`, `docs/CONSOLIDATION_SPRINT_ROADMAP.md` and `docs/LEGAL_INVARIANTS.md`.
> Kept only as history (external review 2026-08-27).

# INSTALL

## Local virtualenv

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements-dev.txt
pip install -e .
cp ops/config.example.yaml ./config.yaml
PRRO_GATEWAY_CONFIG=./config.yaml python scripts/run_rest.py
```

## Docker

```bash
docker build -t prro-gateway:rc1 .
docker compose up --build
```

## systemd

Copy `ops/systemd/prro-gateway-rest.service` to `/etc/systemd/system/` and adjust `WorkingDirectory`, `Environment=PRRO_GATEWAY_CONFIG=...`, and the Python executable path.
