from __future__ import annotations

import sqlite3

from ..enums import InboxStatus, OperationType, Protocol
from ..models.storage import InboxRecord
from ._base import columns_clause, fetchall_model, fetchone_model


class InboxRepository:
    def accept_command(
        conn: sqlite3.Connection,
        *,
        request_id: str,
        idempotency_key: str,
        protocol: Protocol | str,
        operation_type: OperationType | str,
        fiscal_number: str,
        payload_json: str,
        payload_sha256: str,
        route_key: str | None = None,
        backend_profile_id: str | None = None,
        transport_profile_id: str | None = None,
        channel_owner: str | None = None,
        protocol_session_id: str | None = None,
        external_request_id: str | None = None,
        response_deadline_at: str | None = None,
    ) -> tuple[InboxRecord, bool]:
        """Returns (record, is_replay). is_replay=True if the record already existed."""
        try:
            conn.execute(
                """
                INSERT INTO ingress_inbox (
                    request_id, idempotency_key, protocol, operation_type, fiscal_number,
                    route_key, backend_profile_id, transport_profile_id, channel_owner,
                    protocol_session_id, external_request_id, payload_json, payload_sha256,
                    status, response_deadline_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'NEW', ?)
                """,
                (
                    request_id, idempotency_key, str(protocol), str(operation_type), fiscal_number,
                    route_key, backend_profile_id, transport_profile_id, channel_owner,
                    protocol_session_id, external_request_id, payload_json, payload_sha256,
                    response_deadline_at,
                ),
            )
            return InboxRepository.get_by_request_id(conn, request_id), False
        except sqlite3.IntegrityError:
            existing = InboxRepository.get_by_idempotency_key(conn, idempotency_key)
            if existing is not None:
                return existing, True
            raise

    def get_by_request_id(conn: sqlite3.Connection, request_id: str) -> InboxRecord | None:
        return fetchone_model(conn, f"SELECT {columns_clause(InboxRecord)} FROM ingress_inbox WHERE request_id = ?", (request_id,), InboxRecord)

    def get_protocol(conn: sqlite3.Connection, request_id: str) -> str | None:
        """Return the ingress protocol for a document's request_id. Single indexed SELECT."""
        row = conn.execute("SELECT protocol FROM ingress_inbox WHERE request_id = ?", (request_id,)).fetchone()
        return row[0] if row else None

    def get_by_idempotency_key(conn: sqlite3.Connection, idempotency_key: str) -> InboxRecord | None:
        return fetchone_model(conn, f"SELECT {columns_clause(InboxRecord)} FROM ingress_inbox WHERE idempotency_key = ?", (idempotency_key,), InboxRecord)

    def acquire_lease(
        conn: sqlite3.Connection,
        *,
        fiscal_number: str,
        lease_owner: str,
        lease_token: str,
        lease_expires_at: str,
    ) -> InboxRecord | None:
        conn.execute(
            """
            UPDATE ingress_inbox
            SET status = 'PROCESSING',
                lease_owner = ?,
                lease_token = ?,
                lease_expires_at = ?,
                started_at = COALESCE(started_at, CURRENT_TIMESTAMP)
            WHERE request_id = (
                SELECT request_id
                FROM ingress_inbox
                WHERE fiscal_number = ?
                  AND (status = 'NEW' OR (status = 'PROCESSING' AND lease_expires_at IS NOT NULL AND lease_expires_at < CURRENT_TIMESTAMP))
                ORDER BY created_at
                LIMIT 1
            )
            """,
            (lease_owner, lease_token, lease_expires_at, fiscal_number),
        )
        return fetchone_model(
            conn,
            f"SELECT {columns_clause(InboxRecord)} FROM ingress_inbox WHERE lease_token = ? AND lease_owner = ?",
            (lease_token, lease_owner),
            InboxRecord,
        )

    def release_lease(conn: sqlite3.Connection, *, request_id: str, lease_token: str) -> InboxRecord | None:
        conn.execute(
            """
            UPDATE ingress_inbox
            SET lease_owner = NULL,
                lease_token = NULL,
                lease_expires_at = NULL
            WHERE request_id = ? AND lease_token = ?
            """,
            (request_id, lease_token),
        )
        return InboxRepository.get_by_request_id(conn, request_id)

    def requeue(conn: sqlite3.Connection, *, request_id: str, lease_token: str, error_message: str | None = None) -> InboxRecord | None:
        conn.execute(
            """
            UPDATE ingress_inbox
            SET status = 'NEW',
                requeue_count = requeue_count + 1,
                lease_owner = NULL,
                lease_token = NULL,
                lease_expires_at = NULL,
                error_message = ?
            WHERE request_id = ? AND lease_token = ?
            """,
            (error_message, request_id, lease_token),
        )
        return InboxRepository.get_by_request_id(conn, request_id)

    def mark_done(conn: sqlite3.Connection, *, request_id: str, lease_token: str, result_document_id: str | None = None) -> InboxRecord | None:
        conn.execute(
            """
            UPDATE ingress_inbox
            SET status = 'DONE',
                lease_owner = NULL,
                lease_token = NULL,
                lease_expires_at = NULL,
                result_document_id = ?,
                finished_at = CURRENT_TIMESTAMP
            WHERE request_id = ? AND lease_token = ?
            """,
            (result_document_id, request_id, lease_token),
        )
        return InboxRepository.get_by_request_id(conn, request_id)

    def mark_error(
        conn: sqlite3.Connection,
        *,
        request_id: str,
        lease_token: str,
        canonical_error_code: str,
        error_message: str,
    ) -> InboxRecord | None:
        conn.execute(
            """
            UPDATE ingress_inbox
            SET status = 'ERROR',
                lease_owner = NULL,
                lease_token = NULL,
                lease_expires_at = NULL,
                canonical_error_code = ?,
                error_message = ?,
                finished_at = CURRENT_TIMESTAMP
            WHERE request_id = ? AND lease_token = ?
            """,
            (canonical_error_code, error_message, request_id, lease_token),
        )
        return InboxRepository.get_by_request_id(conn, request_id)

    def mark_dead(
        conn: sqlite3.Connection,
        *,
        request_id: str,
        canonical_error_code: str,
        error_message: str,
    ) -> InboxRecord | None:
        conn.execute(
            """
            UPDATE ingress_inbox
            SET status = 'DEAD',
                lease_owner = NULL,
                lease_token = NULL,
                lease_expires_at = NULL,
                canonical_error_code = ?,
                error_message = ?,
                finished_at = CURRENT_TIMESTAMP
            WHERE request_id = ?
            """,
            (canonical_error_code, error_message, request_id),
        )
        return InboxRepository.get_by_request_id(conn, request_id)
