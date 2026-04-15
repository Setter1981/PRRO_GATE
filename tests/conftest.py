from __future__ import annotations

import sqlite3
from pathlib import Path

import pytest

from prro_gateway.migrations.runner import apply_migrations_to_connection

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def sql_root() -> Path:
    return ROOT / 'sql'


@pytest.fixture
def conn(sql_root: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(':memory:')
    apply_migrations_to_connection(connection, sql_root)
    yield connection
    connection.close()
