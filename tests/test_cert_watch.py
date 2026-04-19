"""Sprint 13 — certificate-status monitoring service tests.

Mocks `prro_crypto` at the module boundary so no real network traffic
or native-wheel functions are touched. The fixture `crypto_stub` is
handed to `CertWatchService(crypto_module=...)`.

Frozen invariants verified:
* FI-1: OCSP/CRL calls happen outside the cache write transaction —
        the service opens its own short connection for cache upsert
        AFTER the stubbed crypto call returns.
* FI-4: idempotency — repeated `poll_all_active` stays at a single
        cache row per fiscal_number.
"""
from __future__ import annotations

import hashlib
import sqlite3
import threading
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from prro_gateway.enums import NodeMode
from prro_gateway.migrations.runner import apply_migrations_to_connection
from prro_gateway.repositories import (
    CertStatusRepository,
    CertWatchConfigRepository,
    NodeStateRepository,
)
from prro_gateway.services.cert_watch import (
    STATUS_GOOD,
    STATUS_REVOKED,
    STATUS_UNKNOWN,
    STATUS_UNREACHABLE,
    CertStatus,
    CertWatchService,
    ShiftBlockedError,
)

ROOT = Path(__file__).resolve().parents[1]
FN = 'FN-DEV-0013'
CERT_DER = b'CERT-BYTES-FOR-TESTS'
CERT_FP = hashlib.sha256(CERT_DER).hexdigest()


# ─── shared fixtures ────────────────────────────────────────────────────


@pytest.fixture
def db_file(tmp_path: Path) -> Path:
    """Real file-backed SQLite — needed because the service opens a
    fresh connection per call via the `connect` factory. An in-memory
    connection wouldn't survive the close/reopen pattern."""
    db = tmp_path / 'cert_watch.sqlite3'
    conn = sqlite3.connect(db)
    try:
        apply_migrations_to_connection(conn, ROOT / 'sql')
        conn.commit()
        # Seed a minimal node_state row so _maybe_flip_node_mode has
        # something to work with. The bootstrap migration doesn't create
        # node_state rows on its own.
        _seed_node_state(conn, FN)
        conn.commit()
    finally:
        conn.close()
    # Drop the config cache between tests — it's a module-level cache
    # and the value from the previous test's connection persists.
    CertWatchConfigRepository.invalidate_cache()
    return db


@pytest.fixture
def connect(db_file: Path):
    @contextmanager
    def _connect():
        c = sqlite3.connect(db_file)
        try:
            c.row_factory = sqlite3.Row
            yield c
        finally:
            c.close()
    return _connect


@pytest.fixture
def crypto_stub():
    """Mock of the prro_crypto module used by CertWatchService.

    Tests override individual methods in place (the mock returns a
    MagicMock by default, so a test can just do
    `crypto_stub.fetch_ocsp_response.return_value = b'...'`).
    """
    stub = MagicMock(name='prro_crypto')
    stub.ocsp_url_from_cert.return_value = 'http://ocsp.example.test'
    stub.crl_url_from_cert.return_value = 'http://crl.example.test/test.crl'
    return stub


@pytest.fixture
def service(connect, crypto_stub):
    return CertWatchService(
        connect=connect,
        cert_der_provider=lambda fn: CERT_DER,
        active_fiscal_numbers=lambda: [FN],
        crypto_module=crypto_stub,
    )


def _seed_node_state(conn: sqlite3.Connection, fn: str) -> None:
    conn.execute(
        """
        INSERT INTO node_state (
            node_id, fiscal_number, mode, shift_state,
            next_lnd, readiness_state, recovery_stage,
            current_month_bucket, current_month_offline_seconds,
            next_z_report_number
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            f'node-{fn}', fn, 'ONLINE', 'CREATED',
            1, 'READY', 'DONE',
            '2026-04', 0, 1,
        ),
    )


def _enable_config(connect) -> None:
    with connect() as c:
        c.execute('BEGIN IMMEDIATE')
        CertWatchConfigRepository.update(c, enabled=True)
        c.commit()
    CertWatchConfigRepository.invalidate_cache()


# ─── CS1 — disabled config → all entrypoints pass through ────────────────


def test_cs1_disabled_config_short_circuits(service, crypto_stub):
    # Default migration leaves enabled=0.
    CertWatchConfigRepository.invalidate_cache()
    status = service.check_on_shift_open(FN)
    assert status.status == STATUS_GOOD
    # No OCSP call touched.
    crypto_stub.fetch_ocsp_response.assert_not_called()

    service.poll_all_active()
    crypto_stub.fetch_ocsp_response.assert_not_called()

    service.check_on_dps_error(FN, 'ERROR_VEREFY')
    crypto_stub.fetch_ocsp_response.assert_not_called()

    refreshed = service.force_refresh(FN)
    assert refreshed.status == STATUS_GOOD
    crypto_stub.fetch_ocsp_response.assert_not_called()


# ─── CS2 — fresh cache hit → no OCSP call ────────────────────────────────


def test_cs2_fresh_cache_hit_skips_ocsp(connect, service, crypto_stub):
    _enable_config(connect)
    now_iso = datetime.now(UTC).isoformat(timespec='seconds')
    with connect() as c:
        c.execute('BEGIN IMMEDIATE')
        CertStatusRepository.upsert(
            c, fiscal_number=FN, cert_fingerprint=CERT_FP,
            status=STATUS_GOOD, this_update=now_iso, last_checked_at=now_iso,
            source='ocsp',
        )
        c.commit()

    status = service.check_on_shift_open(FN)
    assert status.status == STATUS_GOOD
    crypto_stub.fetch_ocsp_response.assert_not_called()


# ─── CS3 — stale cache → OCSP refresh updates row ────────────────────────


def test_cs3_stale_cache_triggers_ocsp(connect, service, crypto_stub):
    _enable_config(connect)
    old_iso = '2020-01-01T00:00:00+00:00'
    with connect() as c:
        c.execute('BEGIN IMMEDIATE')
        CertStatusRepository.upsert(
            c, fiscal_number=FN, cert_fingerprint=CERT_FP,
            status=STATUS_GOOD, this_update=old_iso, last_checked_at=old_iso,
            source='ocsp',
        )
        c.commit()

    crypto_stub.fetch_ocsp_response.return_value = b'fake-ocsp-basic'
    crypto_stub.parse_ocsp_status.return_value = {
        'status': 'good',
        'revoked_at': None,
        'this_update': int(datetime.now(UTC).timestamp()),
        'next_update': int(datetime.now(UTC).timestamp()) + 3600,
        'serial_matches': True,
    }

    status = service.check_on_shift_open(FN)
    assert status.status == STATUS_GOOD
    crypto_stub.fetch_ocsp_response.assert_called_once()

    with connect() as c:
        row = CertStatusRepository.get_by_fiscal_number(c, FN)
    assert row is not None
    assert row.status == STATUS_GOOD
    assert row.last_checked_at > old_iso  # ISO timestamps sort lexically


# ─── CS4 — OCSP revoked → shift-open blocks, node → CRYPTO_DEGRADED ──────


def test_cs4_revoked_blocks_shift_open(connect, service, crypto_stub):
    _enable_config(connect)
    crypto_stub.fetch_ocsp_response.return_value = b'fake-ocsp-basic'
    crypto_stub.parse_ocsp_status.return_value = {
        'status': 'revoked',
        'revoked_at': int(datetime(2026, 1, 15, tzinfo=UTC).timestamp()),
        'this_update': int(datetime.now(UTC).timestamp()),
        'next_update': None,
        'serial_matches': True,
    }

    with pytest.raises(ShiftBlockedError) as exc_info:
        service.check_on_shift_open(FN)
    assert exc_info.value.status.is_revoked

    # Cache row now says revoked.
    with connect() as c:
        row = CertStatusRepository.get_by_fiscal_number(c, FN)
    assert row is not None
    assert row.status == STATUS_REVOKED
    assert row.revoked_at is not None

    # Node state flipped to CRYPTO_DEGRADED.
    with connect() as c:
        ns = NodeStateRepository.get_state(c, FN)
    assert NodeMode(ns.mode) == NodeMode.CRYPTO_DEGRADED


def test_cs4b_good_after_revoked_restores_online(connect, service, crypto_stub):
    _enable_config(connect)
    # Force CRYPTO_DEGRADED precondition.
    with connect() as c:
        c.execute('BEGIN IMMEDIATE')
        NodeStateRepository.update_mode(c, fiscal_number=FN, mode=NodeMode.CRYPTO_DEGRADED)
        c.commit()

    crypto_stub.fetch_ocsp_response.return_value = b'fake-ocsp-basic'
    crypto_stub.parse_ocsp_status.return_value = {
        'status': 'good',
        'revoked_at': None,
        'this_update': int(datetime.now(UTC).timestamp()),
        'next_update': int(datetime.now(UTC).timestamp()) + 3600,
        'serial_matches': True,
    }
    service.force_refresh(FN)

    with connect() as c:
        ns = NodeStateRepository.get_state(c, FN)
    assert NodeMode(ns.mode) == NodeMode.ONLINE


# ─── CS5 — OCSP fails + CRL fallback → unreachable, shift NOT blocked ────


def test_cs5_ocsp_fail_crl_fallback_unreachable_not_blocking(connect, service, crypto_stub):
    _enable_config(connect)
    # OCSP raises; CRL fetch also fails → status=unreachable, shift-open not blocked.
    crypto_stub.fetch_ocsp_response.side_effect = RuntimeError('OCSP network down')
    crypto_stub.fetch_crl.side_effect = RuntimeError('CRL fetch failed')

    status = service.check_on_shift_open(FN)  # must NOT raise
    assert status.status == STATUS_UNREACHABLE

    with connect() as c:
        row = CertStatusRepository.get_by_fiscal_number(c, FN)
    assert row is not None
    assert row.status == STATUS_UNREACHABLE
    assert row.last_error is not None


def test_cs5b_ocsp_fail_crl_ok_still_unreachable_safe_fail(connect, service, crypto_stub):
    _enable_config(connect)
    crypto_stub.fetch_ocsp_response.side_effect = RuntimeError('OCSP network down')
    crypto_stub.fetch_crl.return_value = b'\x30\x05\x01\x02\x03\x04\x05'

    status = service.check_on_shift_open(FN)  # must NOT raise
    assert status.status == STATUS_UNREACHABLE  # we don't parse CRL yet
    assert status.source == 'crl'


# ─── CS6 — DPS cert-related error triggers force_refresh ─────────────────


def test_cs6_dps_cert_error_triggers_refresh(connect, service, crypto_stub):
    _enable_config(connect)
    crypto_stub.fetch_ocsp_response.return_value = b'fake-ocsp-basic'
    crypto_stub.parse_ocsp_status.return_value = {
        'status': 'revoked',
        'revoked_at': int(datetime(2026, 1, 15, tzinfo=UTC).timestamp()),
        'this_update': int(datetime.now(UTC).timestamp()),
        'next_update': None,
        'serial_matches': True,
    }
    service.check_on_dps_error(FN, 'ERROR_VEREFY signature invalid')
    crypto_stub.fetch_ocsp_response.assert_called_once()

    with connect() as c:
        row = CertStatusRepository.get_by_fiscal_number(c, FN)
    assert row is not None
    assert row.status == STATUS_REVOKED


def test_cs6b_dps_non_cert_error_is_noop(connect, service, crypto_stub):
    _enable_config(connect)
    service.check_on_dps_error(FN, 'BACKEND_TEMPORARY_UNAVAILABLE: rate limit')
    crypto_stub.fetch_ocsp_response.assert_not_called()


def test_cs6c_dps_numeric_code_classification(connect, service, crypto_stub):
    _enable_config(connect)
    crypto_stub.fetch_ocsp_response.return_value = b'fake-ocsp-basic'
    crypto_stub.parse_ocsp_status.return_value = {
        'status': 'good',
        'revoked_at': None,
        'this_update': int(datetime.now(UTC).timestamp()),
        'next_update': int(datetime.now(UTC).timestamp()) + 3600,
        'serial_matches': True,
    }
    # -14 ERROR_NOT_REGISTERED_SIGNER → cert-related per fiscal_server.proto.
    service.check_on_dps_error(FN, -14)
    crypto_stub.fetch_ocsp_response.assert_called_once()

    crypto_stub.reset_mock()
    # -5 ERROR_TYPE → not cert-related.
    service.check_on_dps_error(FN, -5)
    crypto_stub.fetch_ocsp_response.assert_not_called()


# ─── CS7 — idempotency: repeated poll stays at one row ───────────────────


def test_cs7_idempotency_single_row(connect, service, crypto_stub):
    _enable_config(connect)
    crypto_stub.fetch_ocsp_response.return_value = b'fake-ocsp-basic'
    crypto_stub.parse_ocsp_status.return_value = {
        'status': 'good',
        'revoked_at': None,
        'this_update': int(datetime.now(UTC).timestamp()),
        'next_update': int(datetime.now(UTC).timestamp()) + 3600,
        'serial_matches': True,
    }
    service.poll_all_active()
    service.poll_all_active()
    service.poll_all_active()

    with connect() as c:
        count = c.execute('SELECT COUNT(*) FROM cert_status_cache WHERE fiscal_number = ?', (FN,)).fetchone()[0]
    assert count == 1


# ─── CS8 — cached-only path for write_path (FI-1 compliance) ─────────────


def test_cs8_cached_only_requires_no_network(connect, service, crypto_stub):
    _enable_config(connect)
    # Seed a cached revoked row.
    now_iso = datetime.now(UTC).isoformat(timespec='seconds')
    with connect() as c:
        c.execute('BEGIN IMMEDIATE')
        CertStatusRepository.upsert(
            c, fiscal_number=FN, cert_fingerprint=CERT_FP,
            status=STATUS_REVOKED, this_update=now_iso, last_checked_at=now_iso,
            source='ocsp', revoked_at=now_iso,
        )
        c.commit()

    with pytest.raises(ShiftBlockedError):
        service.check_on_shift_open_cached_only(FN)

    # Absolutely no network calls.
    crypto_stub.fetch_ocsp_response.assert_not_called()
    crypto_stub.fetch_crl.assert_not_called()


def test_cs8b_cached_only_no_row_does_not_block(connect, service, crypto_stub):
    _enable_config(connect)
    status = service.check_on_shift_open_cached_only(FN)
    # No cache row exists yet — safe-fail to unreachable, NOT blocked.
    assert status.status == STATUS_UNREACHABLE
    crypto_stub.fetch_ocsp_response.assert_not_called()
