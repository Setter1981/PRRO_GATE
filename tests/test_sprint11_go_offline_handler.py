"""Sprint 11 — GO_OFFLINE / GO_ONLINE handlers.

Tests:
  OF1: GO_OFFLINE transitions node ONLINE → OFFLINE
  OF2: GO_OFFLINE creates offline_sessions record
  OF3: GO_OFFLINE from non-ONLINE state → NODE_ALREADY_OFFLINE
  OF4: GO_OFFLINE with open shift is allowed (shift ≠ blocker)
  OF5: GO_ONLINE transitions node OFFLINE → ONLINE
  OF6: GO_ONLINE closes offline session and persists duration
  OF7: GO_ONLINE from non-OFFLINE state → NODE_NOT_OFFLINE
  OF8: GO_ONLINE blocked when offline backlog exists
  OF9: Channel lock check skipped for GO_OFFLINE (management op)
"""
from __future__ import annotations

import json
import sqlite3
from datetime import UTC, datetime

import pytest

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


def _next_id(prefix: str) -> str:
    global _seq
    _seq += 1
    return f'{prefix}-s11-{_seq}'


class _StubCrypto:
    def sign(self, *, document_id: str, payload_json: str) -> str:
        return f'signed::{document_id}'


class _StubTransport:
    def send(self, *, document_id: str, signed_payload: str,
             fiscal_number: str, backend_profile_id: str,
             transport_profile_id: str, **kw):
        from prro_gateway.ports import SendResult
        now = datetime.now(UTC)
        return SendResult(
            transport_request_id=f'tx-{document_id}',
            submission_status='ACK',
            server_fiscal_no=f'FISC-{document_id}',
            server_fiscal_date=now.isoformat(),
            response_json=json.dumps({'ok': True}),
            sent_at=now, ack_at=now,
        )


def _worker() -> WritePathWorker:
    return WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_StubTransport())


def _enqueue_mgmt(conn: sqlite3.Connection, op: OperationType, payload: dict | None = None) -> str:
    rid = _next_id('req')
    cmd = CanonicalFiscalCommand(
        request_id=rid,
        idempotency_key=_next_id('idem'),
        protocol=Protocol.CHECKBOX_REST,
        operation_type=op,
        fiscal_number=FN,
        route_key='pos-1',
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner='front-a',
        external_request_id=_next_id('ext'),
        business_ts=datetime(2026, 4, 15, 10, 0, 0, tzinfo=UTC),
        payload=payload or {},
        payload_sha256=_next_id('sha'),
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn,
        request_id=rid,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=FN,
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner='front-a',
        external_request_id=cmd.external_request_id,
        protocol_session_id=None,
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )
    conn.commit()
    return rid


def _set_mode(conn: sqlite3.Connection, mode: NodeMode) -> None:
    conn.execute('BEGIN IMMEDIATE')
    NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=mode)
    conn.commit()


def _create_active_range(conn: sqlite3.Connection, suffix: str = 'default') -> None:
    """Seed an offline code range so GO_OFFLINE guard passes (B1 requirement)."""
    conn.execute('BEGIN IMMEDIATE')
    OfflineRepository.create_range(
        conn, range_id=f'range-{suffix}', fiscal_number=FN,
        first_fiscal_no=1001, last_fiscal_no=1200,
        issued_at='2026-04-15T09:00:00+00:00',
    )
    conn.commit()


# ---------------------------------------------------------------------------
# OF1 — GO_OFFLINE transitions node ONLINE → OFFLINE
# ---------------------------------------------------------------------------

def test_of1_go_offline_transitions_to_offline(conn: sqlite3.Connection) -> None:
    _create_active_range(conn, 'of1')
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue_mgmt(conn, OperationType.GO_OFFLINE)

    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ACK', r
    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.OFFLINE


# ---------------------------------------------------------------------------
# OF2 — GO_OFFLINE creates offline_sessions record
# ---------------------------------------------------------------------------

def test_of2_go_offline_creates_session(conn: sqlite3.Connection) -> None:
    _create_active_range(conn, 'of2')
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue_mgmt(conn, OperationType.GO_OFFLINE, payload={'reason': 'DPS unavailable'})

    _worker().process_next(conn, fiscal_number=FN)

    session = OfflineRepository.get_open_session(conn, FN)
    assert session is not None
    assert session.status.value in ('OPEN', 'OPENING')


# ---------------------------------------------------------------------------
# OF3 — GO_OFFLINE from non-ONLINE state → NODE_ALREADY_OFFLINE
# ---------------------------------------------------------------------------

def test_of3_go_offline_from_offline_rejected(conn: sqlite3.Connection) -> None:
    _set_mode(conn, NodeMode.OFFLINE)
    _enqueue_mgmt(conn, OperationType.GO_OFFLINE)

    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ERROR'
    assert r.canonical_error is not None
    assert r.canonical_error.code == 'NODE_ALREADY_OFFLINE'


# ---------------------------------------------------------------------------
# OF4 — GO_OFFLINE with open shift is allowed
# ---------------------------------------------------------------------------

def test_of4_go_offline_with_open_shift_allowed(conn: sqlite3.Connection) -> None:
    _create_active_range(conn, 'of4')
    _set_mode(conn, NodeMode.ONLINE)
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn, shift_id='shift-of4', fiscal_number=FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST, integration_owner='front-a',
        channel_lock_acquired_at='2026-04-15T10:00:00+00:00',
    )
    conn.commit()

    _enqueue_mgmt(conn, OperationType.GO_OFFLINE)
    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ACK', r
    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.OFFLINE


# ---------------------------------------------------------------------------
# OF5 — GO_ONLINE transitions node OFFLINE → ONLINE
# ---------------------------------------------------------------------------

def test_of5_go_online_transitions_to_online(conn: sqlite3.Connection) -> None:
    _create_active_range(conn, 'of5')
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue_mgmt(conn, OperationType.GO_OFFLINE)
    _worker().process_next(conn, fiscal_number=FN)

    _enqueue_mgmt(conn, OperationType.GO_ONLINE)
    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ACK', r
    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.ONLINE


# ---------------------------------------------------------------------------
# OF6 — GO_ONLINE closes session and persists duration
# ---------------------------------------------------------------------------

def test_of6_go_online_closes_session(conn: sqlite3.Connection) -> None:
    _create_active_range(conn, 'of6')
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue_mgmt(conn, OperationType.GO_OFFLINE)
    _worker().process_next(conn, fiscal_number=FN)

    session_before = OfflineRepository.get_open_session(conn, FN)
    assert session_before is not None

    _enqueue_mgmt(conn, OperationType.GO_ONLINE)
    _worker().process_next(conn, fiscal_number=FN)

    # Session must be CLOSED
    row = conn.execute(
        "SELECT status, ended_at FROM offline_sessions WHERE offline_session_id = ?",
        (session_before.offline_session_id,),
    ).fetchone()
    assert row is not None
    assert row[0] == 'CLOSED'
    assert row[1] is not None  # ended_at populated

    # No open session anymore
    assert OfflineRepository.get_open_session(conn, FN) is None


# ---------------------------------------------------------------------------
# OF7 — GO_ONLINE from non-OFFLINE state → NODE_NOT_OFFLINE
# ---------------------------------------------------------------------------

def test_of7_go_online_from_online_rejected(conn: sqlite3.Connection) -> None:
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue_mgmt(conn, OperationType.GO_ONLINE)

    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ERROR'
    assert r.canonical_error.code == 'NODE_NOT_OFFLINE'


# ---------------------------------------------------------------------------
# OF8 — GO_ONLINE blocked when offline backlog exists
# ---------------------------------------------------------------------------

def test_of8_go_online_blocked_by_offline_backlog(conn: sqlite3.Connection) -> None:
    from prro_gateway.enums import DocumentState
    from prro_gateway.repositories.fiscal_documents import FiscalDocumentRepository

    _set_mode(conn, NodeMode.ONLINE)
    # Seed an offline range and go offline
    conn.execute('BEGIN IMMEDIATE')
    OfflineRepository.create_range(
        conn, range_id='range-of8', fiscal_number=FN,
        first_fiscal_no=9001, last_fiscal_no=9050,
        issued_at='2026-04-15T09:00:00+00:00',
    )
    conn.commit()
    _enqueue_mgmt(conn, OperationType.GO_OFFLINE)
    _worker().process_next(conn, fiscal_number=FN)

    # Manually create an OFFLINE_LOCAL_ACK document (simulates a sold offline receipt)
    conn.execute('BEGIN IMMEDIATE')
    session = OfflineRepository.get_open_session(conn, FN)
    conn.execute(
        """INSERT INTO ingress_inbox
           (request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            external_request_id, payload_json, payload_sha256, status)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?)""",
        ('req-of8-backlog', 'idem-of8', 'CHECKBOX_REST', 'SELL', FN,
         BACKEND, TRANSPORT, 'front-a', 'ext-of8', '{}', 'sha-of8', 'DONE'),
    )
    conn.execute(
        """INSERT INTO fiscal_documents
           (document_id, request_id, fiscal_number, lnd, doc_type,
            backend_profile_id, transport_profile_id, fs_mode, state,
            submission_status, business_ts, payload_json, payload_sha256,
            offline_session_id, offline_fiscal_no)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
        ('doc-of8-1', 'req-of8-backlog', FN, 9999, 'SELL',
         BACKEND, TRANSPORT, 'OFFLINE', DocumentState.OFFLINE_LOCAL_ACK.value,
         'OFFLINE_LOCAL', '2026-04-15T10:05:00+00:00', '{}', 'sha-of8',
         session.offline_session_id, 9001),
    )
    conn.commit()

    _enqueue_mgmt(conn, OperationType.GO_ONLINE)
    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ERROR'
    assert r.canonical_error.code == 'OFFLINE_BACKLOG_NOT_SYNCED'
    # Node must still be OFFLINE
    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.OFFLINE


# ---------------------------------------------------------------------------
# OF9 — Channel lock skipped for GO_OFFLINE (management op)
# ---------------------------------------------------------------------------

def test_of9_channel_lock_not_enforced_for_go_offline(conn: sqlite3.Connection) -> None:
    """GO_OFFLINE must succeed even when command channel differs from shift channel."""
    _create_active_range(conn, 'of9')
    _set_mode(conn, NodeMode.ONLINE)
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn, shift_id='shift-of9', fiscal_number=FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST, integration_owner='front-B',  # different owner
        channel_lock_acquired_at='2026-04-15T10:00:00+00:00',
    )
    conn.commit()

    # Command uses channel_owner='front-a' which differs from shift's 'front-B'
    # For a fiscal op this would return SHIFT_CHANNEL_SWITCH_FORBIDDEN;
    # for GO_OFFLINE the check must be skipped.
    _enqueue_mgmt(conn, OperationType.GO_OFFLINE)
    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ACK', r
