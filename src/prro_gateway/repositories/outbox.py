from __future__ import annotations

import sqlite3


class OutboxRepository:
    def enqueue_document(
        conn: sqlite3.Connection,
        *,
        outbox_id: str,
        aggregate_id: str,
        fiscal_number: str,
        payload_path: str,
        payload_sha256: str,
        sequence_no: int = 0,
        destination: str = 'CLOUD_HUB',
    ) -> None:
        conn.execute(
            """
            INSERT INTO sync_outbox (
                outbox_id, aggregate_type, aggregate_id, fiscal_number, artifact_class,
                sequence_no, destination, payload_path, payload_sha256, status
            ) VALUES (?, 'DOCUMENT', ?, ?, 'DOCUMENT', ?, ?, ?, ?, 'PENDING')
            """,
            (outbox_id, aggregate_id, fiscal_number, sequence_no, destination, payload_path, payload_sha256),
        )


    def count_pending(conn: sqlite3.Connection) -> int:
        row = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE status IN ('PENDING','SENDING')",
        ).fetchone()
        return row[0] if row else 0


__all__ = ['OutboxRepository']
