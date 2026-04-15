"""
Gate 1j — migration idempotency smoke test.

Verifies that running the migration path a second (and third) time on an already
fully-migrated database:
  - returns an empty executed list (nothing re-applied)
  - does not raise any exception
  - does not add duplicate rows to schema_migrations
  - does not alter the schema (key tables remain intact)
  - leaves the checksum records stable (same checksums stored)

Also tests the in-memory (apply_migrations_to_connection) path because the
RuntimeContainer uses that path for auto-migrate on startup — if a container
is started against an already-migrated DB, the in-memory path must also be safe.

No production code changes — this is a pure correctness smoke test.
"""
from __future__ import annotations

import sqlite3
from pathlib import Path

from prro_gateway.migrations.runner import (
    apply_migrations,
    apply_migrations_to_connection,
    applied,
    file_checksum,
)

ROOT = Path(__file__).resolve().parents[1]
SQL_DIR = ROOT / 'sql'

# Tables that must exist after a successful migration (key subset)
EXPECTED_TABLES = {
    'schema_migrations',
    'ingress_inbox',
    'fiscal_documents',
    'shifts',
    'node_state',
    'offline_sessions',
    'offline_ranges',
    'transport_profiles',
    'backend_profiles',
    'audit_log',
}


def _table_names(conn: sqlite3.Connection) -> set[str]:
    rows = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    ).fetchall()
    return {row[0] for row in rows}


# ---------------------------------------------------------------------------
# apply_migrations (file-backed DB path)
# ---------------------------------------------------------------------------

def test_gate1j_second_run_returns_empty_list(tmp_path: Path) -> None:
    """Second apply_migrations call must return [] — nothing re-applied."""
    db_path = tmp_path / 'gate1j.sqlite3'
    first = apply_migrations(db_path, SQL_DIR)
    assert len(first) >= 2, f'First run must apply at least 2 migrations, got {first}'

    second = apply_migrations(db_path, SQL_DIR)
    assert second == [], (
        f'Second run must return [] (idempotent), got {second!r}'
    )


def test_gate1j_third_run_also_empty(tmp_path: Path) -> None:
    """Idempotency holds for repeated calls, not just the second."""
    db_path = tmp_path / 'gate1j_3runs.sqlite3'
    apply_migrations(db_path, SQL_DIR)
    apply_migrations(db_path, SQL_DIR)
    third = apply_migrations(db_path, SQL_DIR)
    assert third == [], f'Third run must return [] (idempotent), got {third!r}'


def test_gate1j_no_duplicate_schema_migration_rows(tmp_path: Path) -> None:
    """schema_migrations must have exactly N rows after N runs, never duplicates."""
    db_path = tmp_path / 'gate1j_dupes.sqlite3'
    apply_migrations(db_path, SQL_DIR)
    apply_migrations(db_path, SQL_DIR)
    apply_migrations(db_path, SQL_DIR)

    conn = sqlite3.connect(db_path)
    try:
        row_count = conn.execute('SELECT COUNT(*) FROM schema_migrations').fetchone()[0]
        migration_files = sorted(SQL_DIR.glob('*.sql'))
        expected_count = len(migration_files)
        assert row_count == expected_count, (
            f'Expected {expected_count} rows in schema_migrations (one per file), '
            f'got {row_count} after 3 runs — duplicates may exist'
        )
    finally:
        conn.close()


def test_gate1j_checksums_stable_after_second_run(tmp_path: Path) -> None:
    """Checksums stored in schema_migrations must not change between runs."""
    db_path = tmp_path / 'gate1j_checksums.sqlite3'
    apply_migrations(db_path, SQL_DIR)

    conn = sqlite3.connect(db_path)
    try:
        checksums_after_first = {
            row[0]: row[1]
            for row in conn.execute(
                'SELECT migration_name, checksum FROM schema_migrations'
            ).fetchall()
        }
    finally:
        conn.close()

    apply_migrations(db_path, SQL_DIR)

    conn = sqlite3.connect(db_path)
    try:
        checksums_after_second = {
            row[0]: row[1]
            for row in conn.execute(
                'SELECT migration_name, checksum FROM schema_migrations'
            ).fetchall()
        }
    finally:
        conn.close()

    assert checksums_after_first == checksums_after_second, (
        'Checksums in schema_migrations changed between runs — '
        f'first={checksums_after_first!r}, second={checksums_after_second!r}'
    )


def test_gate1j_stored_checksums_match_files(tmp_path: Path) -> None:
    """Checksums in schema_migrations must match the actual file checksums."""
    db_path = tmp_path / 'gate1j_file_checksums.sqlite3'
    apply_migrations(db_path, SQL_DIR)

    conn = sqlite3.connect(db_path)
    try:
        stored = {
            row[0]: row[1]
            for row in conn.execute(
                'SELECT migration_name, checksum FROM schema_migrations'
            ).fetchall()
        }
    finally:
        conn.close()

    for sql_file in sorted(SQL_DIR.glob('*.sql')):
        expected = file_checksum(sql_file)
        actual = stored.get(sql_file.name)
        assert actual == expected, (
            f'{sql_file.name}: stored checksum {actual!r} != file checksum {expected!r}'
        )


def test_gate1j_schema_intact_after_second_run(tmp_path: Path) -> None:
    """Key tables must all be present after a repeated migration run."""
    db_path = tmp_path / 'gate1j_schema.sqlite3'
    apply_migrations(db_path, SQL_DIR)
    apply_migrations(db_path, SQL_DIR)

    conn = sqlite3.connect(db_path)
    try:
        tables = _table_names(conn)
    finally:
        conn.close()

    missing = EXPECTED_TABLES - tables
    assert not missing, (
        f'Tables missing after second migration run: {missing!r}'
    )


# ---------------------------------------------------------------------------
# apply_migrations_to_connection (in-memory path — used by RuntimeContainer)
# ---------------------------------------------------------------------------

def test_gate1j_in_memory_second_run_returns_empty() -> None:
    """
    apply_migrations_to_connection (used by RuntimeContainer auto-migrate)
    must also be idempotent when called twice on the same connection.
    """
    conn = sqlite3.connect(':memory:')
    try:
        first = apply_migrations_to_connection(conn, SQL_DIR)
        assert len(first) >= 2, f'First in-memory run must apply migrations, got {first}'

        second = apply_migrations_to_connection(conn, SQL_DIR)
        assert second == [], (
            f'Second in-memory run must return [] (idempotent), got {second!r}'
        )
    finally:
        conn.close()


def test_gate1j_in_memory_applied_set_stable() -> None:
    """applied() set must not grow between repeated in-memory runs."""
    conn = sqlite3.connect(':memory:')
    try:
        apply_migrations_to_connection(conn, SQL_DIR)
        applied_after_first = applied(conn)

        apply_migrations_to_connection(conn, SQL_DIR)
        applied_after_second = applied(conn)

        assert applied_after_first == applied_after_second, (
            f'applied() set changed between runs: '
            f'first={applied_after_first!r}, second={applied_after_second!r}'
        )
    finally:
        conn.close()


def test_gate1j_in_memory_schema_intact() -> None:
    """Key tables must exist in-memory after two migration runs."""
    conn = sqlite3.connect(':memory:')
    try:
        apply_migrations_to_connection(conn, SQL_DIR)
        apply_migrations_to_connection(conn, SQL_DIR)
        tables = _table_names(conn)
    finally:
        conn.close()

    missing = EXPECTED_TABLES - tables
    assert not missing, (
        f'Tables missing after in-memory second migration run: {missing!r}'
    )
