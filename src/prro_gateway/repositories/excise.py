from __future__ import annotations

import sqlite3
from datetime import UTC, datetime

from ..models.storage import ExciseMarkRecord
from ._base import columns_clause, fetchall_model


def normalize_excise_mark(raw: object | None) -> str:
    """Normalize an excise mark: strip, uppercase, remove non-alphanumeric."""
    if raw is None:
        return ''
    return ''.join(ch for ch in str(raw).strip().upper() if ch.isalnum())


class DuplicateExciseMarkError(RuntimeError):
    def __init__(self, normalized_mark_code: str) -> None:
        super().__init__(f'Duplicate excise mark: {normalized_mark_code}')
        self.normalized_mark_code = normalized_mark_code


class ExciseRepository:
    def reserve_marks(
        conn: sqlite3.Connection,
        *,
        document_id: str,
        marks: list[dict],
    ) -> list[ExciseMarkRecord]:
        records: list[ExciseMarkRecord] = []
        for mark in marks:
            normalized = mark['normalized_mark_code']
            try:
                conn.execute(
                    """
                    INSERT INTO excise_marks (
                        excise_mark_id, normalized_mark_code, raw_mark_code, mark_type, status,
                        receipt_document_id, receipt_item_no, product_code
                    ) VALUES (?, ?, ?, ?, 'RESERVED', ?, ?, ?)
                    """,
                    (
                        f"excise-{document_id}-{mark.get('receipt_item_no') or 0}-{normalized}",
                        normalized,
                        mark.get('raw_mark_code', normalized),
                        mark.get('mark_type'),
                        document_id,
                        mark.get('receipt_item_no'),
                        mark.get('product_code'),
                    ),
                )
            except sqlite3.IntegrityError as exc:
                raise DuplicateExciseMarkError(normalized) from exc
        return fetchall_model(conn, f'SELECT {columns_clause(ExciseMarkRecord)} FROM excise_marks WHERE receipt_document_id = ? ORDER BY receipt_item_no, normalized_mark_code', (document_id,), ExciseMarkRecord)

    def mark_document_sold(conn: sqlite3.Connection, *, document_id: str, sold_at: str | None = None) -> None:
        sold_at = sold_at or datetime.now(UTC).isoformat()
        conn.execute(
            """
            UPDATE excise_marks
            SET status = 'SOLD', sold_at = ?, updated_at = CURRENT_TIMESTAMP
            WHERE receipt_document_id = ? AND status = 'RESERVED'
            """,
            (sold_at, document_id),
        )


    def mark_returned(
        conn: sqlite3.Connection,
        *,
        mark_codes: list[str],
        returned_at: str | None = None,
    ) -> int:
        """Transition marks from SOLD to RETURNED. Returns count transitioned."""
        if not mark_codes:
            return 0
        returned_at = returned_at or datetime.now(UTC).isoformat()
        placeholders = ','.join('?' * len(mark_codes))
        cursor = conn.execute(
            f"UPDATE excise_marks SET status = 'RETURNED', returned_at = ?, updated_at = CURRENT_TIMESTAMP "
            f"WHERE normalized_mark_code IN ({placeholders}) AND status = 'SOLD'",
            [returned_at, *mark_codes],
        )
        return cursor.rowcount


__all__ = ['ExciseRepository', 'DuplicateExciseMarkError', 'normalize_excise_mark']
