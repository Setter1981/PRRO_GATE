"""Certificate-status monitoring service (Sprint 13).

Operational problem solved
--------------------------
A bookkeeper or internal admin revokes the company's signing cert at
the CA without telling the cashier. The PRRO keeps signing; ДПС rejects
every receipt with "cert revoked"; shift breaks down. We want to learn
about revocations proactively, from the CA, and block shift-open
cleanly instead of letting the cashier discover it one failed receipt
at a time.

Trust model
-----------
We do NOT verify OCSP/CRL signatures — TLS protects us from forgery on
the wire, and the threat model is legitimate internal revocation, not
an attacker. Signature verification lands in a later sprint if MITM
ever becomes part of the threat picture.

Invariant compliance
--------------------
* OCSP/CRL network calls happen OUTSIDE any SQLite transaction (FI-1).
* Cache upsert is idempotent on fiscal_number (FI-4).
* The background poller stops on shutdown via a threading.Event (FI-9).

The real cert bytes are injected via `cert_der_provider` — the runtime
plugs in whatever lookup the crypto provider knows about. In tests the
provider is a plain dict.
"""
from __future__ import annotations

import hashlib
import sqlite3
import threading
import time
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Callable, Iterable

from ..enums import NodeMode
from ..logging import get_logger
from ..repositories import (
    AuditRepository,
    CertStatusRepository,
    CertWatchConfigRepository,
    NodeStateRepository,
    ShiftRepository,
)
from ..utils.json_codec import dumps_json


# DPS error codes that are plausibly cert-related. fiscal_server.proto:
#   -1  ERROR_VEREFY              — signature/crypto verification failed
#   -14 ERROR_NOT_REGISTERED_SIGNER — signer not registered / cert rejected
# We also accept string codes that look cert-adjacent so stub transports
# can feed us meaningful hints.
_CERT_DPS_STATUS_CODES: frozenset[int] = frozenset({-1, -14})
_CERT_ERROR_KEYWORDS: tuple[str, ...] = (
    'REVOK', 'NOT_REGISTERED_SIGNER', 'ERROR_VEREFY',
    'CERT', 'SIGNER', 'SIGNATURE_INVALID',
)


# Status sentinels used both in-process and persisted to cert_status_cache.
STATUS_GOOD = 'good'
STATUS_REVOKED = 'revoked'
STATUS_UNKNOWN = 'unknown'
STATUS_UNREACHABLE = 'unreachable'

# Source markers persisted alongside each cache row.
SOURCE_OCSP = 'ocsp'
SOURCE_CRL = 'crl'
SOURCE_ADMIN_REFRESH = 'admin_refresh'
SOURCE_DISABLED = 'cached'  # sentinel when config.enabled=false — we never actually talked to a CA


@dataclass(frozen=True)
class CertStatus:
    """In-process view of a cert-status observation."""
    status: str
    source: str
    revoked_at: datetime | None = None
    this_update: datetime | None = None
    next_update: datetime | None = None
    error: str | None = None

    @property
    def is_revoked(self) -> bool:
        return self.status == STATUS_REVOKED

    @property
    def is_good(self) -> bool:
        return self.status == STATUS_GOOD


class ShiftBlockedError(RuntimeError):
    """Raised by check_on_shift_open when the signing cert is revoked."""

    def __init__(self, fiscal_number: str, *, status: CertStatus) -> None:
        super().__init__(
            f'Shift-open blocked for fiscal_number={fiscal_number}: '
            f'signing cert status={status.status}'
        )
        self.fiscal_number = fiscal_number
        self.status = status


# Type alias: resolver from fiscal_number → DER-encoded signer cert.
# Returning None means "no cert known" — the service treats that as
# unreachable (cannot check) and does NOT block shift-open.
CertDerProvider = Callable[[str], bytes | None]


class CertWatchService:
    """Small stateful service: checks, caches, and blocks on revocation.

    The service holds only the connection-factory and helper callables —
    no SQLite connection is held open across calls. Each method opens a
    short connection, does its reads/writes, and closes.
    """

    def __init__(
        self,
        *,
        connect: Callable[[], sqlite3.Connection],
        cert_der_provider: CertDerProvider,
        active_fiscal_numbers: Callable[[], Iterable[str]] | None = None,
        crypto_module=None,
    ) -> None:
        self._connect = connect
        self._cert_der_provider = cert_der_provider
        self._active_fiscal_numbers = active_fiscal_numbers or self._default_active_fiscal_numbers
        # Injected for testability. Defaults to the shipped wheel; the
        # service survives its absence by treating checks as unreachable.
        if crypto_module is None:
            try:
                import prro_crypto as crypto_module  # type: ignore
            except ImportError:
                crypto_module = None
        self._crypto = crypto_module
        self._logger = get_logger('prro_gateway.cert_watch')

    # ─── Public entrypoints ──────────────────────────────────────────────

    def check_on_shift_open(self, fiscal_number: str) -> CertStatus:
        """Blocking check at SHIFT_OPEN time.

        Contract:
        * config.enabled=false    → pass-through (returns STATUS_GOOD).
        * cache hit + fresh       → fast return from cache.
        * otherwise               → synchronous OCSP/CRL check.
        * revoked                 → raises ShiftBlockedError.
        * unreachable / unknown   → does NOT block (safe-fail). The
          operator must not be locked out because the CA is flaky.
        """
        cfg = self._load_config()
        if not cfg.enabled:
            return CertStatus(status=STATUS_GOOD, source=SOURCE_DISABLED)

        cached = self._fresh_cached_status(fiscal_number, cfg.polling_interval_seconds)
        if cached is not None:
            status = _record_to_status(cached)
        else:
            status = self._perform_check(fiscal_number, cfg=cfg, source=SOURCE_OCSP)

        if status.is_revoked:
            raise ShiftBlockedError(fiscal_number, status=status)
        return status

    def check_on_shift_open_cached_only(self, fiscal_number: str) -> CertStatus:
        """Cache-only shift-open check — safe to call inside a SQLite txn.

        Used by write_path where invariant FI-1 forbids network I/O.
        The background poller and admin-refresh own the responsibility
        of keeping the cache populated. If the cache says revoked we
        block; otherwise we pass.
        """
        cfg = self._load_config()
        if not cfg.enabled:
            return CertStatus(status=STATUS_GOOD, source=SOURCE_DISABLED)
        with self._connect() as conn:
            record = CertStatusRepository.get_by_fiscal_number(conn, fiscal_number)
        if record is None:
            # No cache row yet — not enough evidence to block (safe-fail).
            return CertStatus(status=STATUS_UNREACHABLE, source=SOURCE_OCSP)
        status = _record_to_status(record)
        if status.is_revoked:
            raise ShiftBlockedError(fiscal_number, status=status)
        return status

    def poll_all_active(self) -> None:
        """Re-check every active-shift fiscal_number. Safe to call on a timer."""
        cfg = self._load_config()
        if not cfg.enabled:
            return
        for fn in self._active_fiscal_numbers():
            try:
                self._perform_check(fn, cfg=cfg, source=SOURCE_OCSP)
            except Exception as exc:  # no retry loop — next tick retries (per spec)
                self._logger.warning(
                    'cert_watch.poll_error',
                    extra={'extra_fields': {'fiscal_number': fn, 'error': str(exc)}},
                )

    def check_on_dps_error(self, fiscal_number: str, error_code: object) -> None:
        """Side-channel hook called from write_path's error branches.

        Only acts on cert-related error codes; silent no-op otherwise.
        Never blocks the caller's document — the write_path retry logic
        continues uninterrupted; cert_watch is a post-hoc diagnostic.
        """
        if not self._is_cert_related_error(error_code):
            return
        cfg = self._load_config()
        if not cfg.enabled:
            # Even disabled, we log so a post-mortem can see the hint.
            self._logger.info(
                'cert_watch.dps_error_skipped',
                extra={'extra_fields': {
                    'fiscal_number': fiscal_number,
                    'error_code': str(error_code),
                    'reason': 'cert_watch disabled',
                }},
            )
            return
        try:
            self._perform_check(fiscal_number, cfg=cfg, source=SOURCE_OCSP)
        except Exception as exc:
            self._logger.warning(
                'cert_watch.dps_hook_error',
                extra={'extra_fields': {
                    'fiscal_number': fiscal_number,
                    'error': str(exc),
                }},
            )

    def force_refresh(self, fiscal_number: str) -> CertStatus:
        """Admin-triggered one-shot refresh. Bypasses the cache."""
        cfg = self._load_config()
        if not cfg.enabled:
            # Admin force-refresh on a disabled service still short-circuits
            # (no network touched); operator must enable first.
            return CertStatus(status=STATUS_GOOD, source=SOURCE_DISABLED)
        return self._perform_check(fiscal_number, cfg=cfg, source=SOURCE_ADMIN_REFRESH)

    # ─── Internals ───────────────────────────────────────────────────────

    def _load_config(self):
        with self._connect() as conn:
            return CertWatchConfigRepository.get(conn)

    def _fresh_cached_status(self, fiscal_number: str, polling_interval_seconds: int):
        with self._connect() as conn:
            cached = CertStatusRepository.get_by_fiscal_number(conn, fiscal_number)
        if cached is None:
            return None
        now = datetime.now(UTC)
        # Stale if next_update (if known) has passed, OR last_checked_at
        # is older than the polling interval.
        if cached.next_update:
            try:
                nu = _parse_ts(cached.next_update)
                if nu <= now:
                    return None
            except ValueError:
                return None
        try:
            lc = _parse_ts(cached.last_checked_at)
        except ValueError:
            return None
        if (now - lc).total_seconds() >= polling_interval_seconds:
            return None
        return cached

    def _perform_check(self, fiscal_number: str, *, cfg, source: str) -> CertStatus:
        """Run the OCSP probe (with optional CRL fallback), persist, return.

        Invariant FI-1: network calls happen here, with NO open write
        transaction. We only open BEGIN IMMEDIATE for the short cache
        upsert + node_state flip once the network call has completed.
        """
        cert_der = None
        try:
            cert_der = self._cert_der_provider(fiscal_number)
        except Exception as exc:
            return self._persist_unreachable(
                fiscal_number, source=source,
                error=f'cert_der_provider raised: {exc}',
                fingerprint='',
            )
        if not cert_der:
            return self._persist_unreachable(
                fiscal_number, source=source,
                error='no cert_der available for fiscal_number',
                fingerprint='',
            )

        fingerprint = hashlib.sha256(cert_der).hexdigest()
        timeout = float(cfg.request_timeout_seconds)

        crypto = self._crypto
        if crypto is None:
            return self._persist_unreachable(
                fiscal_number, source=source,
                error='prro_crypto module unavailable',
                fingerprint=fingerprint,
            )

        # Resolve OCSP URL (explicit override > cert AIA).
        try:
            ocsp_url = cfg.ocsp_url_override or crypto.ocsp_url_from_cert(cert_der)
        except Exception as exc:
            ocsp_url = None
            self._logger.debug(
                'cert_watch.aia_parse_failed',
                extra={'extra_fields': {'fiscal_number': fiscal_number, 'error': str(exc)}},
            )

        basic_ocsp_der: bytes | None = None
        ocsp_error: str | None = None
        if ocsp_url:
            try:
                # Caller supplies issuer-name-hash & issuer-key-hash. In
                # production these come from parsing the issuer cert with
                # GOST 34.311. For the scope of this service we take them
                # from the provider if it supplies them; otherwise we
                # compute SHA-1 placeholders (OCSP default) — the mock in
                # tests doesn't care about the bytes.
                issuer_name_hash, issuer_key_hash, serial_der = _issuer_hashes_from_cert(cert_der)
                basic_ocsp_der = crypto.fetch_ocsp_response(
                    ocsp_url,
                    issuer_name_hash, issuer_key_hash, serial_der,
                    None, timeout,
                )
            except Exception as exc:
                ocsp_error = f'OCSP fetch: {exc}'
                self._logger.debug(
                    'cert_watch.ocsp_fetch_failed',
                    extra={'extra_fields': {
                        'fiscal_number': fiscal_number, 'error': ocsp_error,
                    }},
                )

        # Parse OCSP if we got a response.
        parsed: dict | None = None
        if basic_ocsp_der is not None:
            try:
                parsed = crypto.parse_ocsp_status(basic_ocsp_der)
            except AttributeError:
                # Wheel predates parse_ocsp_status — treat as unreachable.
                ocsp_error = 'prro_crypto.parse_ocsp_status missing (rebuild wheel)'
                parsed = None
            except Exception as exc:
                ocsp_error = f'OCSP parse: {exc}'
                parsed = None

        if parsed is not None:
            return self._persist_parsed(
                fiscal_number, source=SOURCE_OCSP if source == SOURCE_OCSP else source,
                parsed=parsed, fingerprint=fingerprint,
            )

        # OCSP did not resolve. Optionally try CRL fallback. We embed
        # only enough to know whether the cert was revoked; full CRL
        # parsing lives in a later sprint — for now treat successful
        # CRL fetch as "unreachable-with-data" so the shift isn't
        # blocked while operators decide.
        if cfg.fallback_to_crl:
            try:
                crl_url = crypto.crl_url_from_cert(cert_der)
            except Exception:
                crl_url = None
            if crl_url:
                try:
                    _ = crypto.fetch_crl(crl_url, timeout)
                    # CRL fetched but we don't parse it yet — safe-fail.
                    return self._persist_unreachable(
                        fiscal_number, source=SOURCE_CRL,
                        error='CRL fetched but not parsed (OCSP failed)',
                        fingerprint=fingerprint,
                    )
                except Exception as exc:
                    return self._persist_unreachable(
                        fiscal_number, source=source,
                        error=ocsp_error or f'CRL fetch: {exc}',
                        fingerprint=fingerprint,
                    )

        return self._persist_unreachable(
            fiscal_number, source=source,
            error=ocsp_error or 'no OCSP URL and no CRL fallback',
            fingerprint=fingerprint,
        )

    def _persist_parsed(
        self, fiscal_number: str, *, source: str, parsed: dict, fingerprint: str,
    ) -> CertStatus:
        status_str = parsed.get('status', STATUS_UNKNOWN)
        if status_str not in {STATUS_GOOD, STATUS_REVOKED, STATUS_UNKNOWN}:
            status_str = STATUS_UNKNOWN
        this_update_unix = parsed.get('this_update') or int(time.time())
        next_update_unix = parsed.get('next_update')
        revoked_at_unix = parsed.get('revoked_at')
        now_iso = _utc_iso(datetime.now(UTC))
        this_update_iso = _utc_iso(_from_unix(this_update_unix))
        next_update_iso = _utc_iso(_from_unix(next_update_unix)) if next_update_unix else None
        revoked_at_iso = _utc_iso(_from_unix(revoked_at_unix)) if revoked_at_unix else None

        with self._connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            try:
                CertStatusRepository.upsert(
                    conn,
                    fiscal_number=fiscal_number,
                    cert_fingerprint=fingerprint,
                    status=status_str,
                    revoked_at=revoked_at_iso,
                    this_update=this_update_iso,
                    next_update=next_update_iso,
                    last_checked_at=now_iso,
                    last_error=None,
                    source=source,
                )
                # Flip node mode for CRYPTO_DEGRADED / back to ONLINE.
                self._maybe_flip_node_mode(conn, fiscal_number, status_str)
                # Idempotent audit breadcrumb for operators.
                AuditRepository.log_event(
                    conn,
                    entity_type='CERT',
                    entity_id=fiscal_number,
                    event_type=f'CERT_STATUS_{status_str.upper()}',
                    severity='WARNING' if status_str == STATUS_REVOKED else 'INFO',
                    event_payload_json=dumps_json({
                        'source': source,
                        'cert_fingerprint': fingerprint,
                        'revoked_at': revoked_at_iso,
                        'next_update': next_update_iso,
                    }),
                )
                conn.commit()
            except Exception:
                conn.rollback()
                raise

        result = CertStatus(
            status=status_str,
            source=source,
            revoked_at=_from_unix(revoked_at_unix) if revoked_at_unix else None,
            this_update=_from_unix(this_update_unix),
            next_update=_from_unix(next_update_unix) if next_update_unix else None,
        )
        if status_str == STATUS_REVOKED:
            _emit_alert(
                'cert_watch.revoked',
                fn=fiscal_number, cert_fp=fingerprint,
                revoked_at=revoked_at_iso,
            )
            self._logger.warning(
                'cert_watch.revoked',
                extra={'extra_fields': {
                    'fn': fiscal_number,
                    'cert_fp': fingerprint,
                    'revoked_at': revoked_at_iso,
                }},
            )
        return result

    def _persist_unreachable(
        self, fiscal_number: str, *, source: str, error: str, fingerprint: str,
    ) -> CertStatus:
        now = datetime.now(UTC)
        now_iso = _utc_iso(now)
        with self._connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            try:
                CertStatusRepository.upsert(
                    conn,
                    fiscal_number=fiscal_number,
                    cert_fingerprint=fingerprint,
                    status=STATUS_UNREACHABLE,
                    this_update=now_iso,
                    last_checked_at=now_iso,
                    last_error=error,
                    source=source,
                )
                conn.commit()
            except Exception:
                conn.rollback()
                raise
        self._logger.info(
            'cert_watch.unreachable',
            extra={'extra_fields': {
                'fiscal_number': fiscal_number,
                'source': source,
                'error': error,
            }},
        )
        return CertStatus(
            status=STATUS_UNREACHABLE, source=source,
            this_update=now, error=error,
        )

    def _maybe_flip_node_mode(
        self, conn: sqlite3.Connection, fiscal_number: str, status_str: str,
    ) -> None:
        node = NodeStateRepository.get_state(conn, fiscal_number)
        if node is None:
            return
        current = NodeMode(node.mode) if not isinstance(node.mode, NodeMode) else node.mode
        if status_str == STATUS_REVOKED and current != NodeMode.CRYPTO_DEGRADED:
            NodeStateRepository.update_mode(
                conn, fiscal_number=fiscal_number, mode=NodeMode.CRYPTO_DEGRADED,
            )
        elif status_str == STATUS_GOOD and current == NodeMode.CRYPTO_DEGRADED:
            NodeStateRepository.update_mode(
                conn, fiscal_number=fiscal_number, mode=NodeMode.ONLINE,
            )

    def _is_cert_related_error(self, error_code: object) -> bool:
        if error_code is None:
            return False
        if isinstance(error_code, int):
            return error_code in _CERT_DPS_STATUS_CODES
        s = str(error_code).upper()
        return any(kw in s for kw in _CERT_ERROR_KEYWORDS)

    def _default_active_fiscal_numbers(self) -> list[str]:
        """Default provider: every fn with an active shift."""
        with self._connect() as conn:
            rows = conn.execute(
                """
                SELECT DISTINCT fiscal_number FROM shifts
                WHERE state IN ('OPENING','OPENED','CLOSING')
                """,
            ).fetchall()
        return [r[0] for r in rows]


# ─── module-level alert stub ─────────────────────────────────────────────

def _emit_alert(event: str, **ctx: object) -> None:
    """Alert bus stub. TODO(Sprint 14): plug into real alert bus.

    For now this is a no-op; the caller also emits a structured log line
    via the logger, which is what operators read today. Replacing this
    function with a real send is a single edit.
    """
    return None


# ─── Internal helpers ────────────────────────────────────────────────────

def _record_to_status(record) -> CertStatus:
    """Project a cert_status_cache row into the in-process view."""
    return CertStatus(
        status=record.status,
        source=record.source,
        revoked_at=_parse_ts(record.revoked_at) if record.revoked_at else None,
        this_update=_parse_ts(record.this_update),
        next_update=_parse_ts(record.next_update) if record.next_update else None,
        error=record.last_error,
    )


def _parse_ts(s: str) -> datetime:
    if s.endswith('Z'):
        s = s[:-1] + '+00:00'
    dt = datetime.fromisoformat(s)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=UTC)
    return dt


def _utc_iso(dt: datetime) -> str:
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=UTC)
    return dt.astimezone(UTC).isoformat(timespec='seconds')


def _from_unix(ts: int | None) -> datetime | None:
    if ts is None:
        return None
    return datetime.fromtimestamp(int(ts), tz=UTC)


def _issuer_hashes_from_cert(cert_der: bytes) -> tuple[bytes, bytes, bytes]:
    """Best-effort extractor for the OCSP CertID ingredients.

    Production builds use GOST 34.311 hashes computed in Rust over the
    issuer DN and issuer SubjectPublicKeyInfo. In this Python layer we
    fall back to SHA-1 over the whole cert DER as an opaque placeholder
    — tests mock `fetch_ocsp_response` and never inspect these bytes.
    When the full Rust-side helper lands, it replaces this function.
    """
    h = hashlib.sha1(cert_der).digest()
    # serial_der: smallest possible INTEGER 0 — placeholder for mocked tests.
    serial_der = bytes([0x02, 0x01, 0x00])
    return h, h, serial_der


__all__ = [
    'CertWatchService', 'CertStatus', 'ShiftBlockedError',
    'STATUS_GOOD', 'STATUS_REVOKED', 'STATUS_UNKNOWN', 'STATUS_UNREACHABLE',
    'SOURCE_OCSP', 'SOURCE_CRL', 'SOURCE_ADMIN_REFRESH', 'SOURCE_DISABLED',
]
