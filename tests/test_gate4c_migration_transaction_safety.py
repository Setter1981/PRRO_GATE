"""
Gate 4c — migration transaction safety.

apply_migrations_to_connection() now uses _split_sql_statements() + explicit
BEGIN IMMEDIATE ... COMMIT instead of executescript().

Guarantees:
  - DDL and schema_migrations INSERT are in the same transaction.
  - On failure mid-migration, neither partial DDL nor schema_migrations record
    is committed (SQLite transactional DDL).
  - Already-applied migrations are skipped (idempotency preserved).
  - Normal migration path is not broken (regression).

Four tests:
  A — regression: normal migration applies correctly, schema_migrations record exists
  B — bad SQL mid-migration → RuntimeError propagates, no schema_migrations record
  C — atomicity: after failed migration, tables from that file do not exist
  D — already-applied migration is skipped (no re-run, no duplicate record)
"""
from __future__ import annotations

import sqlite3
from pathlib import Path

import pytest

from prro_gateway.migrations.runner import (
    apply_migrations_to_connection,
    applied,
    ensure_migrations_table,
)

ROOT = Path(__file__).resolve().parents[1]
SQL_DIR = ROOT / 'sql'


# ---------------------------------------------------------------------------
# Test A — regression: normal migration applies cleanly
# ---------------------------------------------------------------------------

def test_gate4c_normal_migration_applies_cleanly(tmp_path: Path) -> None:
    """
    Normal path not broken: apply_migrations_to_connection() applies the real SQL,
    schema_migrations record is present after success.
    """
    db = tmp_path / 'gate4c_a.sqlite3'
    conn = sqlite3.connect(db)
    try:
        executed = apply_migrations_to_connection(conn, SQL_DIR)
    finally:
        conn.close()

    conn2 = sqlite3.connect(db)
    try:
        done = applied(conn2)
    finally:
        conn2.close()

    assert len(executed) > 0, 'Gate 4c: at least one migration must be executed'
    for name in executed:
        assert name in done, (
            f'Gate 4c: executed migration {name!r} must appear in schema_migrations'
        )


# ---------------------------------------------------------------------------
# Test B — bad SQL: RuntimeError propagates, no schema_migrations record
# ---------------------------------------------------------------------------

def test_gate4c_bad_sql_no_partial_record(tmp_path: Path) -> None:
    """
    A synthetic migration with invalid SQL raises an exception.
    After failure, the schema_migrations table must NOT contain the file name —
    confirming that the INSERT was rolled back with the DDL.
    """
    bad_sql_dir = tmp_path / 'sql_bad'
    bad_sql_dir.mkdir()
    bad_file = bad_sql_dir / '001_bad.sql'
    bad_file.write_text(
        "CREATE TABLE good_table (id INTEGER PRIMARY KEY);\n"
        "THIS IS NOT VALID SQL;\n",
        encoding='utf-8',
    )

    db = tmp_path / 'gate4c_b.sqlite3'
    conn = sqlite3.connect(db)
    try:
        ensure_migrations_table(conn)
        with pytest.raises(Exception):
            apply_migrations_to_connection(conn, bad_sql_dir)
        done = applied(conn)
    finally:
        conn.close()

    assert '001_bad.sql' not in done, (
        f'Gate 4c: failed migration must NOT appear in schema_migrations: {done}'
    )


# ---------------------------------------------------------------------------
# Test C — atomicity: partial DDL is also rolled back
# ---------------------------------------------------------------------------

def test_gate4c_failed_migration_leaves_no_partial_ddl(tmp_path: Path) -> None:
    """
    When migration fails mid-execution, CREATE TABLE statements before the
    bad statement must also be rolled back (SQLite transactional DDL).
    After failure, the table created before the bad statement must not exist.
    """
    bad_sql_dir = tmp_path / 'sql_atom'
    bad_sql_dir.mkdir()
    bad_file = bad_sql_dir / '001_atomic.sql'
    bad_file.write_text(
        "CREATE TABLE canary_table (id INTEGER PRIMARY KEY);\n"
        "INVALID STATEMENT THAT WILL FAIL;\n",
        encoding='utf-8',
    )

    db = tmp_path / 'gate4c_c.sqlite3'
    conn = sqlite3.connect(db)
    try:
        ensure_migrations_table(conn)
        with pytest.raises(Exception):
            apply_migrations_to_connection(conn, bad_sql_dir)
        # Check that canary_table does not exist
        tables = {
            row[0] for row in
            conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()
        }
    finally:
        conn.close()

    assert 'canary_table' not in tables, (
        f'Gate 4c: partial DDL must be rolled back on failure, '
        f'but canary_table exists: {tables}'
    )


# ---------------------------------------------------------------------------
# Test D — already-applied migration is skipped
# ---------------------------------------------------------------------------

def test_gate4c_already_applied_migration_is_skipped(tmp_path: Path) -> None:
    """
    Idempotency: if a migration name is already in schema_migrations, it is
    not re-applied. apply_migrations_to_connection() returns an empty list
    on the second call.
    """
    db = tmp_path / 'gate4c_d.sqlite3'
    conn = sqlite3.connect(db)
    try:
        first_run = apply_migrations_to_connection(conn, SQL_DIR)
        assert len(first_run) > 0, 'Gate 4c: first run must apply at least one migration'

        second_run = apply_migrations_to_connection(conn, SQL_DIR)
    finally:
        conn.close()

    assert second_run == [], (
        f'Gate 4c: second run must return empty list (no re-apply): {second_run}'
    )
