from __future__ import annotations

import sqlite3

from ..models.storage import FiscalNumberConfigRecord
from ._base import columns_clause, fetchone_model


class FiscalNumberConfigRepository:
    def get(conn: sqlite3.Connection, fiscal_number: str) -> FiscalNumberConfigRecord | None:
        return fetchone_model(
            conn,
            f'SELECT {columns_clause(FiscalNumberConfigRecord)} FROM fiscal_number_config WHERE fiscal_number = ?',
            (fiscal_number,),
            FiscalNumberConfigRecord,
        )

    def get_or_default(conn: sqlite3.Connection, fiscal_number: str) -> FiscalNumberConfigRecord:
        record = FiscalNumberConfigRepository.get(conn, fiscal_number)
        if record is not None:
            return record
        return FiscalNumberConfigRecord(fiscal_number=fiscal_number)

    def upsert(
        conn: sqlite3.Connection,
        *,
        fiscal_number: str,
        enforce_blocked_mode: int | None = None,
        min_offline_codes: int | None = None,
        max_offline_codes: int | None = None,
    ) -> None:
        existing = FiscalNumberConfigRepository.get(conn, fiscal_number)
        if existing is None:
            em = enforce_blocked_mode if enforce_blocked_mode is not None else 0
            mn = min_offline_codes if min_offline_codes is not None else 0
            mx = max_offline_codes if max_offline_codes is not None else 0
            conn.execute(
                'INSERT INTO fiscal_number_config (fiscal_number, enforce_blocked_mode, min_offline_codes, max_offline_codes) VALUES (?, ?, ?, ?)',
                (fiscal_number, em, mn, mx),
            )
        else:
            em = enforce_blocked_mode if enforce_blocked_mode is not None else existing.enforce_blocked_mode
            mn = min_offline_codes if min_offline_codes is not None else existing.min_offline_codes
            mx = max_offline_codes if max_offline_codes is not None else existing.max_offline_codes
            conn.execute(
                'UPDATE fiscal_number_config SET enforce_blocked_mode = ?, min_offline_codes = ?, max_offline_codes = ?, updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = ?',
                (em, mn, mx, fiscal_number),
            )


__all__ = ['FiscalNumberConfigRepository']
