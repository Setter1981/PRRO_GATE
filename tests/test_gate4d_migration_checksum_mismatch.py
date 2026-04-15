"""
Gate 4d — migration checksum mismatch detection.

apply_migrations_to_connection() now detects when an already-applied migration
file has changed on disk and raises RuntimeError instead of silently skipping.

Four tests:
  A — repeated apply with unchanged file passes (normal startup path)
  B — modified migration file after apply raises RuntimeError with checksum info
  C — RuntimeError message contains both stored and current checksum prefixes
  D — fresh apply of a new migration after a valid existing one still works
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
# Test A — repeated apply with unchanged file passes (no false positive)
# ---------------------------------------------------------------------------

def test_gate4d_repeated_apply_same_file_passes(tmp_path: Path) -> None:
    """
    Normal startup path: apply once, then apply again with same files.
    Must NOT raise — checksums match.
    """
    sql_dir = tmp_path / 'sql_a'
    sql_dir.mkdir()
    f = sql_dir / '001_stable.sql'
    f.write_text("CREATE TABLE stable_table (id INTEGER PRIMARY KEY);", encoding='utf-8')

    db = tmp_path / 'gate4d_a.sqlite3'
    conn = sqlite3.connect(db)
    try:
        apply_migrations_to_connection(conn, sql_dir)
        # Second apply — must pass silently (file unchanged)
        second = apply_migrations_to_connection(conn, sql_dir)
    finally:
        conn.close()

    assert second == [], f'Gate 4d: second run must return empty list, got {second}'


# ---------------------------------------------------------------------------
# Test B — modified migration file after apply raises RuntimeError
# ---------------------------------------------------------------------------

def test_gate4d_modified_migration_raises(tmp_path: Path) -> None:
    """
    If a migration file is modified after it was applied, the next startup
    must raise RuntimeError (not silently skip with stale schema).
    """
    sql_dir = tmp_path / 'sql_b'
    sql_dir.mkdir()
    f = sql_dir / '001_mutable.sql'
    f.write_text("CREATE TABLE original_table (id INTEGER PRIMARY KEY);", encoding='utf-8')

    db = tmp_path / 'gate4d_b.sqlite3'
    conn = sqlite3.connect(db)
    try:
        apply_migrations_to_connection(conn, sql_dir)

        # Simulate post-apply file modification
        f.write_text("CREATE TABLE tampered_table (id INTEGER PRIMARY KEY);", encoding='utf-8')

        with pytest.raises(RuntimeError, match='checksum mismatch'):
            apply_migrations_to_connection(conn, sql_dir)
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# Test C — RuntimeError message contains checksum diagnostic info
# ---------------------------------------------------------------------------

def test_gate4d_mismatch_error_message_contains_checksums(tmp_path: Path) -> None:
    """
    The RuntimeError message must include the migration name and partial checksums
    so operators can diagnose which file drifted.
    """
    sql_dir = tmp_path / 'sql_c'
    sql_dir.mkdir()
    f = sql_dir / '001_diagnosed.sql'
    f.write_text("CREATE TABLE diag_table (id INTEGER PRIMARY KEY);", encoding='utf-8')

    db = tmp_path / 'gate4d_c.sqlite3'
    conn = sqlite3.connect(db)
    try:
        apply_migrations_to_connection(conn, sql_dir)
        f.write_text("CREATE TABLE diag_table_v2 (id INTEGER PRIMARY KEY);", encoding='utf-8')

        with pytest.raises(RuntimeError) as exc_info:
            apply_migrations_to_connection(conn, sql_dir)
    finally:
        conn.close()

    msg = str(exc_info.value)
    assert '001_diagnosed.sql' in msg, f'Gate 4d: migration name must be in error: {msg!r}'
    assert 'stored=' in msg, f'Gate 4d: stored checksum prefix must be in error: {msg!r}'
    assert 'current=' in msg, f'Gate 4d: current checksum prefix must be in error: {msg!r}'


# ---------------------------------------------------------------------------
# Test D — fresh migration after valid existing one applies correctly
# ---------------------------------------------------------------------------

def test_gate4d_new_migration_after_valid_existing_applies(tmp_path: Path) -> None:
    """
    Normal incremental migration: first file applied, second file added later.
    Must apply the second file without raising.
    """
    sql_dir = tmp_path / 'sql_d'
    sql_dir.mkdir()
    f1 = sql_dir / '001_first.sql'
    f1.write_text("CREATE TABLE first_table (id INTEGER PRIMARY KEY);", encoding='utf-8')

    db = tmp_path / 'gate4d_d.sqlite3'
    conn = sqlite3.connect(db)
    try:
        first_run = apply_migrations_to_connection(conn, sql_dir)
        assert first_run == ['001_first.sql'], f'Gate 4d: first run: {first_run}'

        # Add a second migration file
        f2 = sql_dir / '002_second.sql'
        f2.write_text("CREATE TABLE second_table (id INTEGER PRIMARY KEY);", encoding='utf-8')

        second_run = apply_migrations_to_connection(conn, sql_dir)
        assert second_run == ['002_second.sql'], f'Gate 4d: second run should apply only new file: {second_run}'

        done = applied(conn)
    finally:
        conn.close()

    assert '001_first.sql' in done, 'Gate 4d: first migration must remain in applied'
    assert '002_second.sql' in done, 'Gate 4d: second migration must be in applied'
