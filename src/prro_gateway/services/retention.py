"""F2 — Retention/purge service: delete old audit, trace, and inbox rows by TTL.

Invariants preserved:
- FK-safe: ingress_inbox rows referenced by fiscal_documents are NOT deleted.
- No network or crypto calls — pure local I/O.
- Short transactions: each table is purged in its own DELETE statement.
"""
from __future__ import annotations

import sqlite3
from dataclasses import dataclass, field
from datetime import datetime, timedelta, UTC


@dataclass
class PurgeResult:
    success: bool
    audit_deleted: int = 0
    protocol_trace_deleted: int = 0
    transport_trace_deleted: int = 0
    inbox_deleted: int = 0
    error: str | None = None

    @property
    def total_deleted(self) -> int:
        return self.audit_deleted + self.protocol_trace_deleted + self.transport_trace_deleted + self.inbox_deleted


class RetentionService:
    def __init__(
        self,
        audit_ttl_days: int = 90,
        trace_ttl_days: int = 30,
        inbox_ttl_days: int = 90,
    ) -> None:
        self.audit_ttl_days = max(1, audit_ttl_days)
        self.trace_ttl_days = max(1, trace_ttl_days)
        self.inbox_ttl_days = max(1, inbox_ttl_days)

    def run_purge(self, conn: sqlite3.Connection) -> PurgeResult:
        """Delete rows older than the configured TTLs.

        Runs four DELETE statements in a single short transaction.
        ingress_inbox rows referenced by fiscal_documents are skipped (FK-safe).

        Precondition: conn must have no open transaction.
        """
        if conn.in_transaction:
            return PurgeResult(success=False, error='run_purge called with an open transaction on conn')

        now = datetime.now(UTC)
        # Use SQLite CURRENT_TIMESTAMP format (YYYY-MM-DD HH:MM:SS, no T, no tz suffix)
        # to match the DEFAULT CURRENT_TIMESTAMP format used by all audit/trace/inbox tables.
        # Mixing T-format cutoff with space-format DB values causes incorrect lexicographic
        # comparison on the exact boundary day (space 0x20 < T 0x54).
        _fmt = '%Y-%m-%d %H:%M:%S'
        audit_cutoff = (now - timedelta(days=self.audit_ttl_days)).strftime(_fmt)
        trace_cutoff = (now - timedelta(days=self.trace_ttl_days)).strftime(_fmt)
        inbox_cutoff = (now - timedelta(days=self.inbox_ttl_days)).strftime(_fmt)

        try:
            conn.execute('BEGIN IMMEDIATE')

            cur_audit = conn.execute(
                'DELETE FROM audit_log WHERE created_at < ?',
                (audit_cutoff,),
            )
            audit_deleted = cur_audit.rowcount

            cur_proto = conn.execute(
                'DELETE FROM protocol_trace_log WHERE created_at < ?',
                (trace_cutoff,),
            )
            proto_deleted = cur_proto.rowcount

            cur_trans = conn.execute(
                'DELETE FROM transport_trace_log WHERE created_at < ?',
                (trace_cutoff,),
            )
            trans_deleted = cur_trans.rowcount

            # FK-safe: skip inbox rows still referenced by fiscal_documents
            cur_inbox = conn.execute(
                """
                DELETE FROM ingress_inbox
                WHERE status IN ('DONE', 'ERROR', 'DEAD')
                  AND created_at < ?
                  AND NOT EXISTS (
                      SELECT 1 FROM fiscal_documents fd
                      WHERE fd.request_id = ingress_inbox.request_id
                  )
                """,
                (inbox_cutoff,),
            )
            inbox_deleted = cur_inbox.rowcount

            conn.commit()

            return PurgeResult(
                success=True,
                audit_deleted=audit_deleted,
                protocol_trace_deleted=proto_deleted,
                transport_trace_deleted=trans_deleted,
                inbox_deleted=inbox_deleted,
            )

        except Exception as exc:
            try:
                conn.rollback()
            except Exception:
                pass
            return PurgeResult(success=False, error=f'purge failed: {exc}')


__all__ = ['RetentionService', 'PurgeResult']
