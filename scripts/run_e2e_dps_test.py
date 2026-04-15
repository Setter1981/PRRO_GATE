"""E2E DPS test runner — boots runtime with sidecar crypto and test DPS endpoint.

Usage:
    PRRO_GATEWAY_CONFIG=./ops/config.e2e_dps_test.yaml python scripts/run_e2e_dps_test.py

Requires:
    - Sidecar running on port 8091 (JKS_PASSWORD=Jrcfyf123 PORT=8091 node sidecar/server.js)
    - Test fiscal number 4000162280 with shift CLOSED on cabinet.tax.gov.ua:9443
"""
from __future__ import annotations

import sqlite3
from pathlib import Path

import uvicorn

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app


def main() -> None:
    config = AppConfig.from_env()
    container = RuntimeContainer(config)

    # Step 1: migrate
    container.db_path.parent.mkdir(parents=True, exist_ok=True)
    container._ensure_persistent_pragmas()
    if config.database.auto_migrate:
        container._migrate()

    # Step 2: apply e2e seed (DPS test profiles + node state)
    seed_path = Path('ops/seed_e2e_dps_test.sql')
    if seed_path.exists():
        conn = sqlite3.connect(config.database.db_path)
        conn.executescript(seed_path.read_text())
        conn.close()

    # Step 3: wire runtime (picks up seeded profiles)
    container._wire_runtime_services()

    # Production gates not enforced in 'test' environment (by design)
    # Manual health setup
    container.health.ready = True
    container.health.phase = 'RUNNING'

    app = create_app(container)
    uvicorn.run(app, host=config.ingress.rest.host, port=config.ingress.rest.port)


if __name__ == "__main__":
    main()
