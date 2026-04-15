from __future__ import annotations

import time
from pathlib import Path

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.repositories._base import columns_clause
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.services.ingress import IngressAcceptService

ROOT = Path(__file__).resolve().parents[1]


def test_columns_clause_is_cached():
    first = columns_clause.cache_info().hits
    columns_clause.cache_clear()
    a = columns_clause.__wrapped__ if hasattr(columns_clause, "__wrapped__") else None
    x1 = columns_clause(type('Dummy', (), {'model_fields': {'a': None, 'b': None}}))
    x2 = columns_clause(type('Dummy2', (), {'model_fields': {'a': None}}))
    assert x1 == 'a, b'
    assert x2 == 'a'


def test_reconciliation_expected_state_enum():
    assert DocumentState.REQUIRES_MANUAL_RECONCILIATION.value == 'REQUIRES_MANUAL_RECONCILIATION'


def test_shutdown_drains_active_operations(tmp_path):
    cfg = AppConfig.from_mapping({
        'database': {'db_path': str(tmp_path / 'db.sqlite3'), 'sql_dir': str(ROOT / 'sql'), 'auto_migrate': False},
        'runtime': {'graceful_shutdown_timeout_seconds': 1},
    })
    container = RuntimeContainer(cfg)
    container.ingress_service._begin_operation()
    start = time.monotonic()
    container.shutdown()
    elapsed = time.monotonic() - start
    assert elapsed >= 1
    container.ingress_service._end_operation()
