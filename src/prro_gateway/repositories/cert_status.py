"""Repository for the cert_status_cache table.

Stores one row per fiscal_number carrying the most recent OCSP/CRL
observation for that PRRO's signing certificate. Writes happen off the
hot path — only from the cert_watch service — and are idempotent on
(fiscal_number): upsert collapses repeated observations to a single row.
"""
from __future__ import annotations

import sqlite3
from datetime import datetime

from ..models.storage import CertStatusCacheRecord
from ._base import columns_clause, fetchall_model, fetchone_model


class CertStatusRepository:
    def upsert(
        conn: sqlite3.Connection,
        *,
        fiscal_number: str,
        cert_fingerprint: str,
        status: str,
        this_update: str,
        last_checked_at: str,
        source: str,
        revoked_at: str | None = None,
        next_update: str | None = None,
        last_error: str | None = None,
    ) -> CertStatusCacheRecord:
        """Insert or replace the cert-status row for this fiscal_number.

        Idempotent: repeated observations collapse to one row. The caller
        owns the surrounding transaction.
        """
        conn.execute(
            """
            INSERT INTO cert_status_cache
                (fiscal_number, cert_fingerprint, status, revoked_at,
                 this_update, next_update, last_checked_at, last_error, source)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(fiscal_number) DO UPDATE SET
                cert_fingerprint = excluded.cert_fingerprint,
                status = excluded.status,
                revoked_at = excluded.revoked_at,
                this_update = excluded.this_update,
                next_update = excluded.next_update,
                last_checked_at = excluded.last_checked_at,
                last_error = excluded.last_error,
                source = excluded.source
            """,
            (
                fiscal_number, cert_fingerprint, status, revoked_at,
                this_update, next_update, last_checked_at, last_error, source,
            ),
        )
        return CertStatusRepository.get_by_fiscal_number(conn, fiscal_number)

    def get_by_fiscal_number(
        conn: sqlite3.Connection, fiscal_number: str,
    ) -> CertStatusCacheRecord | None:
        return fetchone_model(
            conn,
            f'SELECT {columns_clause(CertStatusCacheRecord)} '
            f'FROM cert_status_cache WHERE fiscal_number = ?',
            (fiscal_number,),
            CertStatusCacheRecord,
        )

    def list_stale(
        conn: sqlite3.Connection,
        *,
        now_iso: str,
        polling_interval_seconds: int,
    ) -> list[CertStatusCacheRecord]:
        """Return cached rows that need a fresh OCSP check.

        A row is stale when either:
          - `next_update` is in the past (CA said "come back by X and we're past X"), or
          - `last_checked_at` is older than `polling_interval_seconds` ago.

        The second condition is a floor — even if the CA gave us a long
        `next_update`, we still re-verify at the polling cadence so a
        revocation that happened in-window doesn't wait a full day to be
        noticed.
        """
        return fetchall_model(
            conn,
            f"""
            SELECT {columns_clause(CertStatusCacheRecord)}
            FROM cert_status_cache
            WHERE (next_update IS NOT NULL AND next_update <= ?)
               OR (CAST(strftime('%s', ?) AS INTEGER)
                   - CAST(strftime('%s', last_checked_at) AS INTEGER) >= ?)
            """,
            (now_iso, now_iso, polling_interval_seconds),
            CertStatusCacheRecord,
        )

    def list_revoked(conn: sqlite3.Connection) -> list[CertStatusCacheRecord]:
        return fetchall_model(
            conn,
            f"SELECT {columns_clause(CertStatusCacheRecord)} "
            f"FROM cert_status_cache WHERE status = 'revoked'",
            (),
            CertStatusCacheRecord,
        )


__all__ = ['CertStatusRepository']
