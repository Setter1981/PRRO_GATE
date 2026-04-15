from __future__ import annotations

import sqlite3

from ..enums import Protocol, ShiftState
from ..models.storage import ChannelLock, ShiftRecord
from ._base import columns_clause, fetchone_model


class ShiftRepository:
    def create_shift(
        conn: sqlite3.Connection,
        *,
        shift_id: str,
        fiscal_number: str,
        state: ShiftState,
        open_mode: str,
        backend_profile_id: str,
        transport_profile_id: str,
        protocol: Protocol,
        integration_owner: str,
        channel_lock_acquired_at: str,
        serial: int | None = None,
        open_document_id: str | None = None,
    ) -> ShiftRecord:
        conn.execute(
            """
            INSERT INTO shifts (
                shift_id, fiscal_number, serial, state, open_mode,
                opened_via_backend_profile_id, opened_via_transport_profile_id,
                opened_via_protocol, opened_via_integration_owner, channel_lock_acquired_at,
                open_document_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                shift_id, fiscal_number, serial, str(state), open_mode,
                backend_profile_id, transport_profile_id, str(protocol), integration_owner,
                channel_lock_acquired_at, open_document_id,
            ),
        )
        return ShiftRepository.get_by_id(conn, shift_id)

    def link_document(
        conn: sqlite3.Connection,
        *,
        shift_id: str,
        open_document_id: str | None = None,
        close_document_id: str | None = None,
        z_report_document_id: str | None = None,
    ) -> None:
        """Set one or more document linkage fields on an existing shift."""
        updates: list[str] = []
        params: list[str] = []
        if open_document_id is not None:
            updates.append('open_document_id = ?')
            params.append(open_document_id)
        if close_document_id is not None:
            updates.append('close_document_id = ?')
            params.append(close_document_id)
        if z_report_document_id is not None:
            updates.append('z_report_document_id = ?')
            params.append(z_report_document_id)
        if not updates:
            return
        params.append(shift_id)
        conn.execute(
            f"UPDATE shifts SET {', '.join(updates)}, updated_at = CURRENT_TIMESTAMP WHERE shift_id = ?",
            params,
        )

    def update_state(conn: sqlite3.Connection, *, shift_id: str, state: ShiftState) -> ShiftRecord | None:
        conn.execute(
            "UPDATE shifts SET state = ?, updated_at = CURRENT_TIMESTAMP WHERE shift_id = ?",
            (str(state), shift_id),
        )
        return ShiftRepository.get_by_id(conn, shift_id)

    def get_by_id(conn: sqlite3.Connection, shift_id: str) -> ShiftRecord | None:
        return fetchone_model(conn, f"SELECT {columns_clause(ShiftRecord)} FROM shifts WHERE shift_id = ?", (shift_id,), ShiftRecord)

    def get_active_shift(conn: sqlite3.Connection, fiscal_number: str) -> ShiftRecord | None:
        return fetchone_model(
            conn,
            f"SELECT {columns_clause(ShiftRecord)} FROM shifts WHERE fiscal_number = ? AND state IN ('OPENING','OPENED','CLOSING') ORDER BY created_at DESC LIMIT 1",
            (fiscal_number,),
            ShiftRecord,
        )

    def get_channel_lock(conn: sqlite3.Connection, fiscal_number: str) -> ChannelLock | None:
        conn.row_factory = sqlite3.Row
        row = conn.execute(
            """
            SELECT opened_via_backend_profile_id AS backend_profile_id,
                   opened_via_transport_profile_id AS transport_profile_id,
                   opened_via_protocol AS protocol,
                   opened_via_integration_owner AS integration_owner,
                   channel_lock_acquired_at AS acquired_at
            FROM shifts
            WHERE fiscal_number = ? AND state IN ('OPENING','OPENED','CLOSING')
            ORDER BY created_at DESC
            LIMIT 1
            """,
            (fiscal_number,),
        ).fetchone()
        if row is None:
            return None
        return ChannelLock.model_validate(dict(row))
