"""Repository for the operator_certs table (Sprint 14 — cert provisioning).

Stores the resolved end-entity signing certificate per fiscal_number.
The cert_provisioning service writes here at onboarding (from the
container directly or via CMP lookup) and the signing path reads for
cert DER by fiscal_number.

Writes are idempotent on fiscal_number (FI-4): `upsert` collapses
repeated provisioning of the same key material to a single row.

FI-1: this repo never initiates network I/O. Callers open short write
transactions around `upsert` only after any CMP round-trip has already
completed outside the transaction.
"""
from __future__ import annotations

import sqlite3
from datetime import UTC, datetime, timedelta

from ..models.storage import (
    CertProvisioningConfigRecord,
    OperatorCertRecord,
)
from ._base import columns_clause, fetchall_model, fetchone_model


class OperatorCertsRepository:
    def upsert(
        conn: sqlite3.Connection,
        *,
        fiscal_number: str,
        cert_fingerprint: str,
        ski_hex: str,
        cert_der: bytes,
        source: str,
        fetched_at: str,
        subject_dn: str | None = None,
        issuer_dn: str | None = None,
        valid_from: str | None = None,
        valid_to: str | None = None,
        last_refresh_at: str | None = None,
    ) -> OperatorCertRecord:
        """Insert or replace the operator cert row for this fiscal_number.

        Caller owns the surrounding transaction. Idempotent: repeated
        calls with identical arguments collapse to one row.
        """
        conn.execute(
            """
            INSERT INTO operator_certs
                (fiscal_number, cert_fingerprint, ski_hex, cert_der,
                 subject_dn, issuer_dn, valid_from, valid_to,
                 fetched_at, source, last_refresh_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(fiscal_number) DO UPDATE SET
                cert_fingerprint = excluded.cert_fingerprint,
                ski_hex = excluded.ski_hex,
                cert_der = excluded.cert_der,
                subject_dn = excluded.subject_dn,
                issuer_dn = excluded.issuer_dn,
                valid_from = excluded.valid_from,
                valid_to = excluded.valid_to,
                fetched_at = excluded.fetched_at,
                source = excluded.source,
                last_refresh_at = excluded.last_refresh_at
            """,
            (
                fiscal_number, cert_fingerprint, ski_hex, cert_der,
                subject_dn, issuer_dn, valid_from, valid_to,
                fetched_at, source, last_refresh_at,
            ),
        )
        result = OperatorCertsRepository.get_by_fiscal_number(conn, fiscal_number)
        assert result is not None  # just upserted
        return result

    def get_by_fiscal_number(
        conn: sqlite3.Connection, fiscal_number: str,
    ) -> OperatorCertRecord | None:
        return fetchone_model(
            conn,
            f'SELECT {columns_clause(OperatorCertRecord)} '
            f'FROM operator_certs WHERE fiscal_number = ?',
            (fiscal_number,),
            OperatorCertRecord,
        )

    def get_by_ski(
        conn: sqlite3.Connection, ski_hex: str,
    ) -> OperatorCertRecord | None:
        return fetchone_model(
            conn,
            f'SELECT {columns_clause(OperatorCertRecord)} '
            f'FROM operator_certs WHERE ski_hex = ?',
            (ski_hex.upper(),),
            OperatorCertRecord,
        )

    def list_expiring(
        conn: sqlite3.Connection,
        *,
        within_days: int,
        now: datetime | None = None,
    ) -> list[OperatorCertRecord]:
        """Return rows whose `valid_to` falls inside [now, now+within_days].

        Rows with no `valid_to` (legacy / parse failure at ingest time)
        are omitted — they can't be scheduled for refresh on a time
        basis. An admin force-refresh is the right tool for those.
        """
        if now is None:
            now = datetime.now(UTC)
        cutoff = (now + timedelta(days=int(within_days))).isoformat(timespec='seconds')
        now_iso = now.isoformat(timespec='seconds')
        return fetchall_model(
            conn,
            f"""
            SELECT {columns_clause(OperatorCertRecord)}
            FROM operator_certs
            WHERE valid_to IS NOT NULL
              AND valid_to <= ?
              AND valid_to >= ?
            """,
            (cutoff, now_iso),
            OperatorCertRecord,
        )


class CertProvisioningConfigRepository:
    def get(conn: sqlite3.Connection) -> CertProvisioningConfigRecord:
        row = fetchone_model(
            conn,
            f'SELECT {columns_clause(CertProvisioningConfigRecord)} '
            f'FROM cert_provisioning_config WHERE id = 1',
            (),
            CertProvisioningConfigRecord,
        )
        if row is None:
            # Migration 015 seeds a row; fallback defends against the
            # edge case of a freshly-migrated test DB that somehow missed
            # the INSERT (e.g. checksum-failed migration path).
            row = CertProvisioningConfigRecord()
        return row


__all__ = [
    'OperatorCertsRepository',
    'CertProvisioningConfigRepository',
]
