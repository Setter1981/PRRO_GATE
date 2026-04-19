"""Sprint 12 — FiscalNumberConfigRepository tests.

FC1: fiscal_number_config table exists after migration
FC2: get() returns None when no record
FC3: get_or_default() returns all-default object when no record
FC4: upsert() inserts new record correctly
FC5: upsert() updates only specified fields
FC6: SQLite constraint rejects max_offline_codes < min_offline_codes
"""
from __future__ import annotations

import sqlite3

import pytest

from prro_gateway.repositories.fn_config import FiscalNumberConfigRepository

FN = 'FN-DEV-0001'


# ---------------------------------------------------------------------------
# FC1 — table exists
# ---------------------------------------------------------------------------

def test_fc1_table_exists_after_migration(conn: sqlite3.Connection) -> None:
    row = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='fiscal_number_config'"
    ).fetchone()
    assert row is not None, "fiscal_number_config table not found"


# ---------------------------------------------------------------------------
# FC2 — get() returns None for missing record
# ---------------------------------------------------------------------------

def test_fc2_get_returns_none_for_missing(conn: sqlite3.Connection) -> None:
    result = FiscalNumberConfigRepository.get(conn, FN)
    assert result is None


# ---------------------------------------------------------------------------
# FC3 — get_or_default() returns defaults when no record
# ---------------------------------------------------------------------------

def test_fc3_get_or_default_returns_defaults(conn: sqlite3.Connection) -> None:
    cfg = FiscalNumberConfigRepository.get_or_default(conn, FN)
    assert cfg.fiscal_number == FN
    assert cfg.enforce_blocked_mode == 0
    assert cfg.min_offline_codes == 0
    assert cfg.max_offline_codes == 0


# ---------------------------------------------------------------------------
# FC4 — upsert() inserts new record
# ---------------------------------------------------------------------------

def test_fc4_upsert_inserts_new_record(conn: sqlite3.Connection) -> None:
    conn.execute('BEGIN IMMEDIATE')
    FiscalNumberConfigRepository.upsert(
        conn,
        fiscal_number=FN,
        enforce_blocked_mode=1,
        min_offline_codes=10,
        max_offline_codes=100,
    )
    conn.commit()

    cfg = FiscalNumberConfigRepository.get(conn, FN)
    assert cfg is not None
    assert cfg.enforce_blocked_mode == 1
    assert cfg.min_offline_codes == 10
    assert cfg.max_offline_codes == 100


# ---------------------------------------------------------------------------
# FC5 — upsert() partial update preserves unspecified fields
# ---------------------------------------------------------------------------

def test_fc5_upsert_partial_update(conn: sqlite3.Connection) -> None:
    conn.execute('BEGIN IMMEDIATE')
    FiscalNumberConfigRepository.upsert(conn, fiscal_number=FN, min_offline_codes=20, max_offline_codes=50)
    conn.commit()

    conn.execute('BEGIN IMMEDIATE')
    FiscalNumberConfigRepository.upsert(conn, fiscal_number=FN, enforce_blocked_mode=1)
    conn.commit()

    cfg = FiscalNumberConfigRepository.get(conn, FN)
    assert cfg.enforce_blocked_mode == 1
    assert cfg.min_offline_codes == 20   # preserved
    assert cfg.max_offline_codes == 50   # preserved


# ---------------------------------------------------------------------------
# FC6 — max_offline_codes < min_offline_codes → constraint violation
# ---------------------------------------------------------------------------

def test_fc6_constraint_max_must_be_gte_min(conn: sqlite3.Connection) -> None:
    with pytest.raises(Exception):
        conn.execute('BEGIN IMMEDIATE')
        conn.execute(
            "INSERT INTO fiscal_number_config (fiscal_number, min_offline_codes, max_offline_codes) VALUES (?, ?, ?)",
            (FN, 50, 10),  # max < min — violates constraint
        )
        conn.commit()
