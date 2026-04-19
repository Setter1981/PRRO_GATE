"""Single-row cert_watch_config repository with a short in-process TTL.

The poller ticks often (every polling_interval_seconds, but the process
also does incidental reads on shift-open / on DPS error hooks). Hitting
SQLite for every read is needlessly noisy — the config changes rarely,
almost always via an admin API. A 5-second process-local cache is
enough to flatten the bursty reads without hiding operator edits for
any meaningful length of time.

The cache is invalidated explicitly by `update(...)` in the same
process. A config write made by a different process would be picked up
within 5 seconds.
"""
from __future__ import annotations

import sqlite3
import threading
import time

from ..models.storage import CertWatchConfigRecord
from ._base import columns_clause, fetchone_model


_CACHE_TTL_SECONDS = 5.0


class CertWatchConfigRepository:
    _cache_lock = threading.Lock()
    _cache_value: CertWatchConfigRecord | None = None
    _cache_expires_at: float = 0.0

    def get(conn: sqlite3.Connection) -> CertWatchConfigRecord:
        """Return the single config row.

        Short-TTL in-process cache avoids hammering SQLite on every
        poll tick. Returned object is a copy; safe to stash.
        """
        now = time.monotonic()
        with CertWatchConfigRepository._cache_lock:
            cached = CertWatchConfigRepository._cache_value
            expires = CertWatchConfigRepository._cache_expires_at
            if cached is not None and now < expires:
                return cached
        row = fetchone_model(
            conn,
            f'SELECT {columns_clause(CertWatchConfigRecord)} FROM cert_watch_config WHERE id = 1',
            (),
            CertWatchConfigRecord,
        )
        if row is None:
            # Migration 013 seeds a row; defensive fallback for freshly-
            # migrated test DBs that somehow missed the INSERT.
            row = CertWatchConfigRecord()
        with CertWatchConfigRepository._cache_lock:
            CertWatchConfigRepository._cache_value = row
            CertWatchConfigRepository._cache_expires_at = now + _CACHE_TTL_SECONDS
        return row

    def update(
        conn: sqlite3.Connection,
        *,
        enabled: bool | int | None = None,
        polling_interval_seconds: int | None = None,
        request_timeout_seconds: int | None = None,
        ocsp_url_override: str | None = None,
        fallback_to_crl: bool | int | None = None,
    ) -> CertWatchConfigRecord:
        """Partial update on the single cert_watch_config row.

        Caller owns the surrounding transaction. Invalidates the
        in-process cache so subsequent `get()` reflects the change.
        """
        updates: list[str] = []
        params: list[object] = []
        if enabled is not None:
            updates.append('enabled = ?')
            params.append(1 if bool(enabled) else 0)
        if polling_interval_seconds is not None:
            updates.append('polling_interval_seconds = ?')
            params.append(int(polling_interval_seconds))
        if request_timeout_seconds is not None:
            updates.append('request_timeout_seconds = ?')
            params.append(int(request_timeout_seconds))
        if ocsp_url_override is not None:
            updates.append('ocsp_url_override = ?')
            # Empty string clears the override.
            params.append(ocsp_url_override or None)
        if fallback_to_crl is not None:
            updates.append('fallback_to_crl = ?')
            params.append(1 if bool(fallback_to_crl) else 0)
        if not updates:
            return CertWatchConfigRepository.get(conn)
        conn.execute(
            f'UPDATE cert_watch_config SET {", ".join(updates)}, '
            f'updated_at = CURRENT_TIMESTAMP WHERE id = 1',
            params,
        )
        CertWatchConfigRepository.invalidate_cache()
        return fetchone_model(
            conn,
            f'SELECT {columns_clause(CertWatchConfigRecord)} FROM cert_watch_config WHERE id = 1',
            (),
            CertWatchConfigRecord,
        )

    def invalidate_cache() -> None:
        """Drop the in-process cache; next `get()` hits SQLite."""
        with CertWatchConfigRepository._cache_lock:
            CertWatchConfigRepository._cache_value = None
            CertWatchConfigRepository._cache_expires_at = 0.0


__all__ = ['CertWatchConfigRepository']
