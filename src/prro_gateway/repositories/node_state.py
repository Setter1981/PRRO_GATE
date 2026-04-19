from __future__ import annotations

import sqlite3

from ..enums import NodeMode
from ..models.storage import NodeStateRecord
from ._base import columns_clause, fetchone_model


class NodeStateRepository:
    def get_state(conn: sqlite3.Connection, fiscal_number: str) -> NodeStateRecord | None:
        return fetchone_model(conn, f"SELECT {columns_clause(NodeStateRecord)} FROM node_state WHERE fiscal_number = ?", (fiscal_number,), NodeStateRecord)

    def update_mode(conn: sqlite3.Connection, *, fiscal_number: str, mode: NodeMode) -> NodeStateRecord | None:
        conn.execute(
            "UPDATE node_state SET mode = ?, updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = ?",
            (str(mode), fiscal_number),
        )
        return NodeStateRepository.get_state(conn, fiscal_number)

    def increment_lnd(conn: sqlite3.Connection, *, fiscal_number: str) -> int:
        """Atomically allocate the next Local Number of Document (LND) for this fiscal_number.

        LND is allocated BEFORE the sign/send attempt. If the subsequent send fails and the
        document is retried, the SAME LND is reused (via the ERROR_RETRYABLE / SIGNED resume
        paths in write_path.py). If the process crashes AFTER document creation but BEFORE
        retry resolution, the allocated LND may become a gap in the sequence.

        DPS specification permits gaps in LND sequences: offline documents create gaps
        naturally (offline_fiscal_no is separate from online LND), and fiscal numbering
        continuity is enforced at the Z-report / shift level, not per-document.
        Gaps are therefore expected, legal, and do NOT require correction.
        """
        row = conn.execute(
            """
            UPDATE node_state
            SET next_lnd = next_lnd + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE fiscal_number = ?
            RETURNING next_lnd - 1 AS allocated_lnd
            """,
            (fiscal_number,),
        ).fetchone()
        if row is None:
            raise LookupError(f"node_state not found for fiscal_number={fiscal_number}")
        return int(row[0])

    def allocate_z_report_number(conn: sqlite3.Connection, *, fiscal_number: str) -> int:
        """Atomically allocate and return the next Z-report number for this fiscal_number."""
        row = conn.execute(
            """
            UPDATE node_state
            SET next_z_report_number = next_z_report_number + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE fiscal_number = ?
            RETURNING next_z_report_number - 1 AS allocated_z
            """,
            (fiscal_number,),
        ).fetchone()
        if row is None:
            raise LookupError(f"node_state not found for fiscal_number={fiscal_number}")
        return int(row[0])

    def update_last_known_mac(conn: sqlite3.Connection, *, fiscal_number: str, mac: str) -> None:
        """Persist DPS-provided MAC hash for bootstrap recovery."""
        conn.execute(
            "UPDATE node_state SET last_known_mac = ?, updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = ?",
            (mac, fiscal_number),
        )

    def update_last_cash_balance(conn: sqlite3.Connection, *, fiscal_number: str, balance: int) -> None:
        """Write carry-over cash balance snapshot at Z_REPORT boundary."""
        conn.execute(
            "UPDATE node_state SET last_cash_balance = ?, updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = ?",
            (balance, fiscal_number),
        )

    def update_last_backup_at(conn: sqlite3.Connection, *, fiscal_number: str, timestamp: str) -> None:
        """Record timestamp of last successful backup for this fiscal number."""
        conn.execute(
            "UPDATE node_state SET last_backup_at = ?, updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = ?",
            (timestamp, fiscal_number),
        )

    def update_offline_seconds(conn: sqlite3.Connection, *, fiscal_number: str, month_bucket: str, seconds_delta: int) -> NodeStateRecord | None:
        conn.execute(
            """
            UPDATE node_state
            SET current_month_bucket = ?,
                current_month_offline_seconds = CASE
                    WHEN current_month_bucket = ? THEN current_month_offline_seconds + ?
                    ELSE ?
                END,
                updated_at = CURRENT_TIMESTAMP
            WHERE fiscal_number = ?
            """,
            (month_bucket, month_bucket, seconds_delta, max(0, seconds_delta), fiscal_number),
        )
        return NodeStateRepository.get_state(conn, fiscal_number)
