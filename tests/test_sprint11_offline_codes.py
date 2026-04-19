"""Sprint 11 — ASK_OFFLINE_CODES handler.

Tests:
  OC1: Creates ACTIVE range, usable immediately
  OC2: Overlapping range rejected → OFFLINE_RANGE_CONFLICT
  OC3: Invalid range (first >= last) rejected
  OC4: Missing fields rejected
  OC5: Range exhaustion marks status EXHAUSTED, next alloc raises
  OC6: Channel lock skipped for ASK_OFFLINE_CODES
"""
from __future__ import annotations

import sqlite3
from datetime import UTC, datetime

from prro_gateway.enums import NodeMode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json

FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'

_seq = 0


def _nid(p: str) -> str:
    global _seq
    _seq += 1
    return f'{p}-oc-{_seq}'


class _NullCrypto:
    def sign(self, *, document_id: str, payload_json: str) -> str:
        return f'sig::{document_id}'


class _NullTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        now = datetime.now(UTC)
        return SendResult(transport_request_id='tx', submission_status='ACK',
                          server_fiscal_no='SFN', server_fiscal_date=now.isoformat(),
                          response_json='{}', sent_at=now, ack_at=now)


def _worker() -> WritePathWorker:
    return WritePathWorker(crypto_provider=_NullCrypto(), transport_client=_NullTransport())


def _ask(conn: sqlite3.Connection, payload: dict) -> object:
    rid = _nid('req')
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=_nid('idem'),
        protocol=Protocol.CHECKBOX_REST,
        operation_type=OperationType.ASK_OFFLINE_CODES,
        fiscal_number=FN, route_key='pos-1',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        channel_owner='front-a', external_request_id=_nid('ext'),
        business_ts=datetime(2026, 4, 15, 10, 0, 0, tzinfo=UTC),
        payload=payload, payload_sha256=_nid('sha'),
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=rid, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=FN, backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT, channel_owner='front-a',
        external_request_id=cmd.external_request_id,
        protocol_session_id=None,
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )
    conn.commit()
    return _worker().process_next(conn, fiscal_number=FN)


# ---------------------------------------------------------------------------
# OC1 — Creates ACTIVE range, usable immediately
# ---------------------------------------------------------------------------

def test_oc1_creates_active_range(conn: sqlite3.Connection) -> None:
    r = _ask(conn, {'first_fiscal_no': 1001, 'last_fiscal_no': 1100})

    assert r.outcome == 'ACK', r
    rng = OfflineRepository.get_active_range(conn, FN)
    assert rng is not None
    assert rng.first_fiscal_no == 1001
    assert rng.last_fiscal_no == 1100
    assert rng.next_fiscal_no == 1001
    assert rng.status == 'ACTIVE'


def test_oc1b_range_usable_immediately(conn: sqlite3.Connection) -> None:
    _ask(conn, {'first_fiscal_no': 2001, 'last_fiscal_no': 2050})

    conn.execute('BEGIN IMMEDIATE')
    code = OfflineRepository.allocate_offline_code(conn, FN)
    conn.commit()

    assert code == 2001


# ---------------------------------------------------------------------------
# OC2 — Overlapping range rejected
# ---------------------------------------------------------------------------

def test_oc2_overlapping_range_rejected(conn: sqlite3.Connection) -> None:
    _ask(conn, {'first_fiscal_no': 3001, 'last_fiscal_no': 3100})

    # Overlapping range [3050, 3200]
    r = _ask(conn, {'first_fiscal_no': 3050, 'last_fiscal_no': 3200})

    assert r.outcome == 'ERROR'
    assert r.canonical_error.code == 'OFFLINE_RANGE_CONFLICT'


# ---------------------------------------------------------------------------
# OC3 — Invalid range (first >= last) rejected
# ---------------------------------------------------------------------------

def test_oc3_invalid_range_first_gte_last(conn: sqlite3.Connection) -> None:
    r = _ask(conn, {'first_fiscal_no': 5000, 'last_fiscal_no': 5000})

    assert r.outcome == 'ERROR'
    assert r.canonical_error.code == 'INVALID_RECEIPT_DATA'


def test_oc3b_first_greater_than_last(conn: sqlite3.Connection) -> None:
    r = _ask(conn, {'first_fiscal_no': 5100, 'last_fiscal_no': 5000})

    assert r.outcome == 'ERROR'
    assert r.canonical_error.code == 'INVALID_RECEIPT_DATA'


# ---------------------------------------------------------------------------
# OC4 — Missing fields rejected
# ---------------------------------------------------------------------------

def test_oc4_missing_first_fiscal_no(conn: sqlite3.Connection) -> None:
    r = _ask(conn, {'last_fiscal_no': 6100})

    assert r.outcome == 'ERROR'
    assert r.canonical_error.code == 'INVALID_RECEIPT_DATA'


def test_oc4b_empty_payload(conn: sqlite3.Connection) -> None:
    r = _ask(conn, {})

    assert r.outcome == 'ERROR'
    assert r.canonical_error.code == 'INVALID_RECEIPT_DATA'


# ---------------------------------------------------------------------------
# OC5 — Exhaustion: allocate all codes, verify EXHAUSTED status
# ---------------------------------------------------------------------------

def test_oc5_range_exhaustion(conn: sqlite3.Connection) -> None:
    _ask(conn, {'first_fiscal_no': 7001, 'last_fiscal_no': 7003})

    conn.execute('BEGIN IMMEDIATE')
    assert OfflineRepository.allocate_offline_code(conn, FN) == 7001
    assert OfflineRepository.allocate_offline_code(conn, FN) == 7002
    assert OfflineRepository.allocate_offline_code(conn, FN) == 7003
    conn.commit()

    row = conn.execute(
        "SELECT status FROM offline_ranges WHERE first_fiscal_no = 7001",
    ).fetchone()
    assert row[0] == 'EXHAUSTED'


def test_oc5b_exhausted_range_raises_on_next_alloc(conn: sqlite3.Connection) -> None:
    import pytest
    _ask(conn, {'first_fiscal_no': 8001, 'last_fiscal_no': 8002})

    conn.execute('BEGIN IMMEDIATE')
    OfflineRepository.allocate_offline_code(conn, FN)
    OfflineRepository.allocate_offline_code(conn, FN)
    conn.commit()

    conn.execute('BEGIN IMMEDIATE')
    with pytest.raises(LookupError):
        OfflineRepository.allocate_offline_code(conn, FN)
    conn.rollback()


# ---------------------------------------------------------------------------
# OC6 — Channel lock skipped for ASK_OFFLINE_CODES
# ---------------------------------------------------------------------------

def test_oc6_channel_lock_not_enforced(conn: sqlite3.Connection) -> None:
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn, shift_id='shift-oc6', fiscal_number=FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST, integration_owner='front-B',
        channel_lock_acquired_at='2026-04-15T10:00:00+00:00',
    )
    conn.commit()

    # Command channel_owner='front-a' ≠ shift's 'front-B', but should not block
    r = _ask(conn, {'first_fiscal_no': 9001, 'last_fiscal_no': 9100})

    assert r.outcome == 'ACK', r
