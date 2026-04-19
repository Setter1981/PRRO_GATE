from __future__ import annotations

import sqlite3

from ..models.storage import OfflineRangeRecord, OfflineSessionRecord
from ._base import columns_clause, fetchone_model


class OfflineRepository:
    def get_open_session(conn: sqlite3.Connection, fiscal_number: str) -> OfflineSessionRecord | None:
        return fetchone_model(
            conn,
            f"""
            SELECT {columns_clause(OfflineSessionRecord)} FROM offline_sessions
            WHERE fiscal_number = ? AND status IN ('OPENING','OPEN','CLOSING')
            ORDER BY created_at DESC
            LIMIT 1
            """,
            (fiscal_number,),
            OfflineSessionRecord,
        )

    def get_active_range(conn: sqlite3.Connection, fiscal_number: str) -> OfflineRangeRecord | None:
        return fetchone_model(
            conn,
            f"""
            SELECT {columns_clause(OfflineRangeRecord)} FROM offline_ranges
            WHERE fiscal_number = ? AND status = 'ACTIVE' AND next_fiscal_no <= last_fiscal_no
            ORDER BY created_at ASC
            LIMIT 1
            """,
            (fiscal_number,),
            OfflineRangeRecord,
        )

    def allocate_offline_code(conn: sqlite3.Connection, fiscal_number: str) -> int:
        row = conn.execute(
            """
            UPDATE offline_ranges
            SET next_fiscal_no = next_fiscal_no + 1,
                updated_at = CURRENT_TIMESTAMP,
                status = CASE WHEN next_fiscal_no + 1 > last_fiscal_no THEN 'EXHAUSTED' ELSE status END
            WHERE range_id = (
                SELECT range_id
                FROM offline_ranges
                WHERE fiscal_number = ? AND status = 'ACTIVE' AND next_fiscal_no <= last_fiscal_no
                ORDER BY created_at ASC
                LIMIT 1
            )
            RETURNING next_fiscal_no - 1 AS allocated_offline_no
            """,
            (fiscal_number,),
        ).fetchone()
        if row is None:
            raise LookupError(f'No active offline code range for fiscal_number={fiscal_number}')
        return int(row[0])

    def create_open_session(
        conn: sqlite3.Connection,
        *,
        offline_session_id: str,
        fiscal_number: str,
        started_at: str,
        status: str = 'OPEN',
        accumulated_month_seconds: int = 0,
        start_fiscal_no: int | None = None,
        reason: str | None = None,
    ) -> OfflineSessionRecord:
        conn.execute(
            """
            INSERT INTO offline_sessions (
                offline_session_id, fiscal_number, started_at, status, accumulated_month_seconds, start_fiscal_no, reason
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (offline_session_id, fiscal_number, started_at, status, accumulated_month_seconds, start_fiscal_no, reason),
        )
        return OfflineRepository.get_open_session(conn, fiscal_number)

    def close_session(
        conn: sqlite3.Connection,
        *,
        offline_session_id: str,
        ended_at: str,
        continuous_seconds: int,
    ) -> None:
        """Close an offline session and persist its duration."""
        conn.execute(
            """
            UPDATE offline_sessions
            SET status = 'CLOSED',
                ended_at = ?,
                accumulated_month_seconds = accumulated_month_seconds + ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE offline_session_id = ?
            """,
            (ended_at, max(0, continuous_seconds), offline_session_id),
        )

    def create_range(
        conn: sqlite3.Connection,
        *,
        range_id: str,
        fiscal_number: str,
        first_fiscal_no: int,
        last_fiscal_no: int,
        issued_at: str,
        source_payload_json: str | None = None,
    ) -> OfflineRangeRecord:
        """Insert a new ACTIVE offline range for this fiscal_number."""
        conn.execute(
            """
            INSERT INTO offline_ranges
                (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
                 next_fiscal_no, issued_at, status, source_payload_json)
            VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE', ?)
            """,
            (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
             first_fiscal_no, issued_at, source_payload_json),
        )
        return OfflineRepository.get_active_range(conn, fiscal_number)

    def has_overlapping_range(
        conn: sqlite3.Connection,
        *,
        fiscal_number: str,
        first_fiscal_no: int,
        last_fiscal_no: int,
    ) -> bool:
        """Return True if any ACTIVE range overlaps the given interval."""
        row = conn.execute(
            """
            SELECT 1 FROM offline_ranges
            WHERE fiscal_number = ?
              AND status = 'ACTIVE'
              AND first_fiscal_no <= ?
              AND last_fiscal_no >= ?
            LIMIT 1
            """,
            (fiscal_number, last_fiscal_no, first_fiscal_no),
        ).fetchone()
        return row is not None

    def count_available(conn: sqlite3.Connection, fiscal_number: str) -> int:
        """Count total unused offline fiscal number codes across all ACTIVE ranges."""
        row = conn.execute(
            """
            SELECT COALESCE(SUM(last_fiscal_no - next_fiscal_no + 1), 0)
            FROM offline_ranges
            WHERE fiscal_number = ? AND status = 'ACTIVE'
            """,
            (fiscal_number,),
        ).fetchone()
        return int(row[0]) if row else 0


__all__ = ['OfflineRepository']
