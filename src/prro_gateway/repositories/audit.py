from __future__ import annotations

import sqlite3

from ..models.storage import AuditLogRecord
from ._base import columns_clause, fetchone_model


class AuditRepository:
    def log_event(
        conn: sqlite3.Connection,
        *,
        entity_type: str,
        entity_id: str,
        event_type: str,
        severity: str = 'INFO',
        event_payload_json: str | None = None,
    ) -> AuditLogRecord:
        cur = conn.execute(
            """
            INSERT INTO audit_log (entity_type, entity_id, event_type, severity, event_payload_json)
            VALUES (?, ?, ?, ?, ?)
            """,
            (entity_type, entity_id, event_type, severity, event_payload_json),
        )
        return fetchone_model(conn, f'SELECT {columns_clause(AuditLogRecord)} FROM audit_log WHERE audit_id = ?', (cur.lastrowid,), AuditLogRecord)

    def log_events(
        conn: sqlite3.Connection,
        *,
        events: list[tuple[str, str, str, str, str | None]],
    ) -> list[AuditLogRecord]:
        if not events:
            return []
        placeholders = ", ".join(["(?, ?, ?, ?, ?)"] * len(events))
        params: list[object | None] = []
        for entity_type, entity_id, event_type, severity, event_payload_json in events:
            params.extend([entity_type, entity_id, event_type, severity, event_payload_json])
        conn.execute(
            f"INSERT INTO audit_log (entity_type, entity_id, event_type, severity, event_payload_json) VALUES {placeholders}",
            tuple(params),
        )
        cur = conn.execute("SELECT audit_id FROM audit_log ORDER BY audit_id DESC LIMIT ?", (len(events),))
        ids = [row[0] for row in cur.fetchall()][::-1]
        return [
            fetchone_model(conn, f'SELECT {columns_clause(AuditLogRecord)} FROM audit_log WHERE audit_id = ?', (audit_id,), AuditLogRecord)
            for audit_id in ids
        ]


__all__ = ['AuditRepository']
