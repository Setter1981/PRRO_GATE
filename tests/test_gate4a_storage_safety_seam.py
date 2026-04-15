"""
Gate 4a — storage safety audit: seam verification tests.

Audit findings:
  - WAL mode + PRAGMA synchronous=FULL + foreign_keys + busy_timeout: already present
  - DB open failure (corrupt file): was unhandled sqlite3.DatabaseError → now RuntimeError
  - PRAGMA quick_check: now called during _ensure_persistent_pragmas() at startup
  - Migration not transactional: documented gap (not fixed in this step)
  - last_integrity_check_at / last_backup_at in schema: dead code (never written)
  - No backup mechanism: documented gap (not fixed in this step)

Four tests:
  A — fresh DB initializes cleanly, quick_check returns 'ok', WAL mode active
  B — corrupted DB file → RuntimeError at initialize() (not unhandled DatabaseError)
  C — quick_check on a running DB via connect() returns 'ok' (seam available)
  D — WAL mode persists when RuntimeContainer is re-created on same DB path
"""
from __future__ import annotations

from pathlib import Path

import httpx

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app
from fastapi.testclient import TestClient
import pytest

ROOT = Path(__file__).resolve().parents[1]


def _config(tmp_path: Path, db_name: str) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'gate4a',
        },
        'runtime': {'process_immediately': False, 'reconcile_on_startup': False},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE4A-LIC',
            'cashier_pin': '0000',
        },
    })


def _strict_no_http(request: httpx.Request) -> httpx.Response:
    raise AssertionError(f'gate4a: unexpected HTTP {request.method} {request.url}')


# ---------------------------------------------------------------------------
# Test A — fresh DB: initialize() succeeds, quick_check 'ok', WAL active
# ---------------------------------------------------------------------------

def test_gate4a_fresh_db_initializes_cleanly(tmp_path: Path) -> None:
    """
    Fresh DB path: _ensure_persistent_pragmas() completes without error.
    After initialize(): WAL mode active, quick_check returns 'ok' via connect().
    Documents that the baseline safety seam is functional.
    """
    cfg = _config(tmp_path, 'gate4a_a.sqlite3')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )
    with TestClient(create_app(c)):
        pass  # initialize() called in lifespan

    with c.connect() as conn:
        journal = conn.execute("PRAGMA journal_mode").fetchone()[0]
        sync = conn.execute("PRAGMA synchronous").fetchone()[0]
        fk = conn.execute("PRAGMA foreign_keys").fetchone()[0]
        busy = conn.execute("PRAGMA busy_timeout").fetchone()[0]
        qc_row = conn.execute("PRAGMA quick_check").fetchone()

    assert str(journal).lower() == 'wal', f'Gate 4a: journal_mode must be WAL: {journal!r}'
    assert int(sync) == 2, f'Gate 4a: synchronous must be FULL (2): {sync!r}'
    assert int(fk) == 1, f'Gate 4a: foreign_keys must be ON: {fk!r}'
    assert int(busy) == 5000, f'Gate 4a: busy_timeout must be 5000: {busy!r}'
    assert qc_row is not None and qc_row[0] == 'ok', (
        f'Gate 4a: quick_check must return ok on fresh DB: {qc_row!r}'
    )


# ---------------------------------------------------------------------------
# Test B — corrupted DB file → RuntimeError (not unhandled sqlite3.DatabaseError)
# ---------------------------------------------------------------------------

def test_gate4a_corrupt_db_raises_runtime_error(tmp_path: Path) -> None:
    """
    A garbage-bytes file at db_path is not a valid SQLite database.
    _ensure_persistent_pragmas() catches sqlite3.DatabaseError and re-raises
    as RuntimeError with a clear diagnostic message.

    BEFORE Gate 4a: this raised unhandled sqlite3.DatabaseError — crash at startup
    with no health signal and no operator-readable context.
    AFTER Gate 4a: RuntimeError('Database unavailable ...') — clear message, same crash
    but catchable by orchestrator wrapper or future health state handler.

    The gap of 'no health.phase=ERROR transition' is documented but not fixed here.
    """
    db_path = tmp_path / 'gate4a_corrupt.sqlite3'
    db_path.write_bytes(b'this is not a sqlite database\x00\xff\xfe' * 100)

    cfg = AppConfig.from_mapping({
        'database': {
            'db_path': str(db_path),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': False,  # skip migration; corruption detected before that
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'gate4a',
        },
        'runtime': {'process_immediately': False, 'reconcile_on_startup': False},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE4A-LIC',
            'cashier_pin': '0000',
        },
    })
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )

    with pytest.raises(RuntimeError, match='Database unavailable'):
        with TestClient(create_app(c)):
            pass


# ---------------------------------------------------------------------------
# Test C — quick_check via connect() on running DB returns 'ok'
# ---------------------------------------------------------------------------

def test_gate4a_quick_check_via_connect_returns_ok(tmp_path: Path) -> None:
    """
    After normal initialization, PRAGMA quick_check is accessible via the
    standard connect() context manager and returns 'ok'.
    Documents the seam for future periodic integrity monitoring.
    """
    cfg = _config(tmp_path, 'gate4a_c.sqlite3')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )
    with TestClient(create_app(c)):
        with c.connect() as conn:
            rows = conn.execute("PRAGMA quick_check").fetchall()

    assert len(rows) == 1 and rows[0][0] == 'ok', (
        f'Gate 4a: quick_check via connect() must return single ok row: {rows!r}'
    )


# ---------------------------------------------------------------------------
# Test D — WAL mode persists when container is re-created on same DB path
# ---------------------------------------------------------------------------

def test_gate4a_wal_mode_persists_across_container_restart(tmp_path: Path) -> None:
    """
    WAL mode is a persistent DB-level setting (stored in the DB file header).
    After shutting down container c1 and creating c2 on the same DB path,
    journal_mode is still WAL — not reset to default DELETE mode.
    Documents that WAL durability invariant holds across restart.
    """
    cfg = _config(tmp_path, 'gate4a_d.sqlite3')

    # First container: initializes DB with WAL
    c1 = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )
    with TestClient(create_app(c1)):
        pass

    # Second container: same DB path, no re-init of journal_mode needed
    c2 = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )
    with TestClient(create_app(c2)):
        with c2.connect() as conn:
            journal = conn.execute("PRAGMA journal_mode").fetchone()[0]

    assert str(journal).lower() == 'wal', (
        f'Gate 4a: WAL mode must persist after container restart: got {journal!r}'
    )
