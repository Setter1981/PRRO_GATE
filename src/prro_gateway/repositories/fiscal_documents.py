from __future__ import annotations

import sqlite3

from ..enums import DocumentState
from ..models.storage import FiscalDocumentRecord
from ._base import columns_clause, fetchall_model, fetchone_model


class FiscalDocumentRepository:
    def create_prepared(
        conn: sqlite3.Connection,
        *,
        document_id: str,
        request_id: str,
        fiscal_number: str,
        lnd: int,
        doc_type: str,
        backend_profile_id: str,
        transport_profile_id: str,
        fs_mode: str,
        business_ts: str,
        payload_json: str,
        payload_sha256: str,
        shift_id: str | None = None,
        offline_session_id: str | None = None,
        receipt_type: str | None = None,
        previous_hash: str | None = None,
        channel_lock_ref: str | None = None,
        offline_fiscal_no: int | None = None,
        offline_fiscal_date: str | None = None,
        related_receipt_id: str | None = None,
        previous_receipt_id: str | None = None,
        technical_return: bool | None = None,
        delivery_json: str | None = None,
        rounding_enabled: bool | None = None,
        total_sum: int | None = None,
        round_sum: int | None = None,
        discounts_sum: int | None = None,
        extra_charge_sum: int | None = None,
    ) -> FiscalDocumentRecord:
        conn.execute(
            """
            INSERT INTO fiscal_documents (
                document_id, request_id, fiscal_number, shift_id, offline_session_id, lnd,
                doc_type, state, backend_profile_id, transport_profile_id, fs_mode,
                receipt_type, business_ts, payload_json, payload_sha256, previous_hash, channel_lock_ref,
                offline_fiscal_no, offline_fiscal_date, related_receipt_id, previous_receipt_id,
                technical_return, delivery_json, rounding_enabled, total_sum, round_sum,
                discounts_sum, extra_charge_sum
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 'PREPARED', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                document_id, request_id, fiscal_number, shift_id, offline_session_id, lnd,
                doc_type, backend_profile_id, transport_profile_id, fs_mode,
                receipt_type, business_ts, payload_json, payload_sha256, previous_hash, channel_lock_ref,
                offline_fiscal_no, offline_fiscal_date, related_receipt_id, previous_receipt_id,
                int(technical_return) if technical_return is not None else None,
                delivery_json,
                int(rounding_enabled) if rounding_enabled is not None else None,
                total_sum, round_sum, discounts_sum, extra_charge_sum,
            ),
        )
        return FiscalDocumentRepository.get_by_id(conn, document_id)

    def update_state(
        conn: sqlite3.Connection,
        *,
        document_id: str,
        state: DocumentState,
        response_json: str | None = None,
        canonical_error_code: str | None = None,
        error_message: str | None = None,
        submission_status: str | None = None,
        transport_request_id: str | None = None,
        server_fiscal_no: str | None = None,
        server_fiscal_date: str | None = None,
        sent_at: str | None = None,
        ack_at: str | None = None,
        expected_states: tuple[DocumentState | str, ...] | None = None,
    ) -> FiscalDocumentRecord | None:
        params = [state.value, response_json, canonical_error_code, error_message, submission_status, transport_request_id, server_fiscal_no, server_fiscal_date, sent_at, ack_at, document_id]
        query = """
            UPDATE fiscal_documents
            SET state = ?,
                response_json = COALESCE(?, response_json),
                canonical_error_code = COALESCE(?, canonical_error_code),
                error_message = COALESCE(?, error_message),
                submission_status = COALESCE(?, submission_status),
                transport_request_id = COALESCE(?, transport_request_id),
                server_fiscal_no = COALESCE(?, server_fiscal_no),
                server_fiscal_date = COALESCE(?, server_fiscal_date),
                sent_at = COALESCE(?, sent_at),
                ack_at = COALESCE(?, ack_at),
                updated_at = CURRENT_TIMESTAMP
            WHERE document_id = ?
        """
        if expected_states:
            normalized = [s.value if isinstance(s, DocumentState) else str(s) for s in expected_states]
            placeholders = ','.join('?' for _ in normalized)
            query += f" AND state IN ({placeholders})"
            params.extend(normalized)
        cur = conn.execute(query, tuple(params))
        if cur.rowcount == 0:
            return None
        return FiscalDocumentRepository.get_by_id(conn, document_id)

    def get_by_id(conn: sqlite3.Connection, document_id: str) -> FiscalDocumentRecord | None:
        return fetchone_model(conn, f'SELECT {columns_clause(FiscalDocumentRecord)} FROM fiscal_documents WHERE document_id = ?', (document_id,), FiscalDocumentRecord)

    def get_by_request_id(conn: sqlite3.Connection, request_id: str) -> FiscalDocumentRecord | None:
        return fetchone_model(conn, f'SELECT {columns_clause(FiscalDocumentRecord)} FROM fiscal_documents WHERE request_id = ?', (request_id,), FiscalDocumentRecord)

    def get_pending_for_reconciliation(conn: sqlite3.Connection, fiscal_number: str | None = None) -> list[FiscalDocumentRecord]:
        if fiscal_number is not None:
            return fetchall_model(
                conn,
                f"""
                SELECT {columns_clause(FiscalDocumentRecord)} FROM fiscal_documents
                WHERE state IN ('SENT','KVT1','KVT2','ERROR_RETRYABLE','REQUIRES_MANUAL_RECONCILIATION')
                  AND fiscal_number = ?
                ORDER BY created_at
                """,
                (fiscal_number,),
                FiscalDocumentRecord,
            )
        return fetchall_model(
            conn,
            f"""
            SELECT {columns_clause(FiscalDocumentRecord)} FROM fiscal_documents
            WHERE state IN ('SENT','KVT1','KVT2','ERROR_RETRYABLE','REQUIRES_MANUAL_RECONCILIATION')
            ORDER BY created_at
            """,
            (),
            FiscalDocumentRecord,
        )

    def get_pending_for_offline_sync(
        conn: sqlite3.Connection,
        fiscal_number: str | None = None,
    ) -> list[FiscalDocumentRecord]:
        """Return OFFLINE_LOCAL_ACK documents awaiting DPS transmission.

        Ordering: offline_fiscal_no ASC, lnd ASC, created_at ASC.
        This preserves the fiscal sequence required by DPS for offline document
        submission — lower offline_fiscal_no must be transmitted first.

        Pass fiscal_number to restrict to a single write-path scope (preferred
        for single-writer callers). Omit for audit/monitoring queries.
        """
        if fiscal_number is not None:
            return fetchall_model(
                conn,
                f"""
                SELECT {columns_clause(FiscalDocumentRecord)} FROM fiscal_documents
                WHERE state = 'OFFLINE_LOCAL_ACK'
                  AND submission_status = 'OFFLINE_LOCAL'
                  AND fiscal_number = ?
                ORDER BY offline_fiscal_no ASC, lnd ASC, created_at ASC
                """,
                (fiscal_number,),
                FiscalDocumentRecord,
            )
        return fetchall_model(
            conn,
            f"""
            SELECT {columns_clause(FiscalDocumentRecord)} FROM fiscal_documents
            WHERE state = 'OFFLINE_LOCAL_ACK'
              AND submission_status = 'OFFLINE_LOCAL'
            ORDER BY offline_fiscal_no ASC, lnd ASC, created_at ASC
            """,
            (),
            FiscalDocumentRecord,
        )

    def count_pending_for_offline_sync(conn: sqlite3.Connection, *, fiscal_number: str | None = None) -> int:
        """Return count of OFFLINE_LOCAL_ACK documents awaiting DPS transmission."""
        if fiscal_number is not None:
            row = conn.execute(
                "SELECT COUNT(*) FROM fiscal_documents"
                " WHERE state = 'OFFLINE_LOCAL_ACK' AND submission_status = 'OFFLINE_LOCAL'"
                " AND fiscal_number = ?",
                (fiscal_number,),
            ).fetchone()
        else:
            row = conn.execute(
                "SELECT COUNT(*) FROM fiscal_documents"
                " WHERE state = 'OFFLINE_LOCAL_ACK' AND submission_status = 'OFFLINE_LOCAL'",
            ).fetchone()
        return row[0] if row else 0

    def count_pending_for_reconciliation(conn: sqlite3.Connection) -> int:
        row = conn.execute(
            "SELECT COUNT(*) FROM fiscal_documents WHERE state IN ('SENT','KVT1','KVT2','ERROR_RETRYABLE')",
        ).fetchone()
        return row[0] if row else 0

    def count_requires_manual_reconciliation(conn: sqlite3.Connection) -> int:
        row = conn.execute(
            "SELECT COUNT(*) FROM fiscal_documents WHERE state = 'REQUIRES_MANUAL_RECONCILIATION'",
        ).fetchone()
        return row[0] if row else 0

    def get_requires_manual_reconciliation(conn: sqlite3.Connection) -> list[FiscalDocumentRecord]:
        return fetchall_model(
            conn,
            f"""
            SELECT {columns_clause(FiscalDocumentRecord)} FROM fiscal_documents
            WHERE state = 'REQUIRES_MANUAL_RECONCILIATION'
            ORDER BY updated_at
            """,
            (),
            FiscalDocumentRecord,
        )

    def set_recovery_attempts(conn: sqlite3.Connection, *, document_id: str, recovery_attempts: int) -> FiscalDocumentRecord | None:
        conn.execute(
            "UPDATE fiscal_documents SET recovery_attempts = ?, updated_at = CURRENT_TIMESTAMP WHERE document_id = ?",
            (recovery_attempts, document_id),
        )
        return FiscalDocumentRepository.get_by_id(conn, document_id)


__all__ = ['FiscalDocumentRepository']
