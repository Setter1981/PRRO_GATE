"""Gate 5B — OfflineSession state transitions: OPEN → CLOSED after GO_ONLINE.

Verifies that the offline session record is properly finalized when the node
goes back online: status=CLOSED, ended_at set, accumulated_month_seconds updated.

Three targeted tests:
  1. test_go_online_closes_offline_session  — status=CLOSED, no open session remains
  2. test_go_online_sets_ended_at           — ended_at is populated
  3. test_go_online_updates_node_month_seconds — current_month_offline_seconds increases,
                                                 node mode returns to ONLINE

Invariants preserved:
  #1 (no network in tx): GO_ONLINE management path has no network/crypto calls.
  #5 (offline limits): session duration is accumulated correctly.
  #8 (recovery must not violate state transitions): OPEN → CLOSED is the only valid path.
"""
from __future__ import annotations

import json
import sqlite3
from datetime import UTC, datetime

from prro_gateway.enums import NodeMode, OfflineSessionState, OperationType, Protocol
from prro_gateway.models.canonical import CanonicalFiscalCommand
from prro_gateway.repositories import InboxRepository
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json

FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'

_seq: list[int] = [0]


def _uid(prefix: str) -> str:
    _seq[0] += 1
    return f'{prefix}-g5b-{_seq[0]}'


# ---------------------------------------------------------------------------
# Stubs
# ---------------------------------------------------------------------------

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
            sent_at=now,
            ack_at=now,
        )


def _worker() -> WritePathWorker:
    return WritePathWorker(
        crypto_provider=_StubCrypto(),
        transport_client=_StubTransport(),
        crypto_breaker_threshold=0,
    )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _enqueue(conn: sqlite3.Connection, op: OperationType) -> str:
    rid = _uid('req')
    cmd = CanonicalFiscalCommand(
        request_id=rid,
        idempotency_key=_uid('idem'),
        protocol=Protocol.CHECKBOX_REST,
        operation_type=op,
        fiscal_number=FN,
        route_key='pos-1',
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner='front-a',
        external_request_id=_uid('ext'),
        business_ts=datetime(2026, 4, 17, 10, 0, 0, tzinfo=UTC),
        payload={},
        payload_sha256=_uid('sha'),
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


def _go_offline(conn: sqlite3.Connection) -> str:
    """Transition node to OFFLINE via the write path, returning the open session id."""
    conn.execute('BEGIN IMMEDIATE')
    OfflineRepository.create_range(
        conn,
        range_id=_uid('range'),
        fiscal_number=FN,
        first_fiscal_no=3001,
        last_fiscal_no=3200,
        issued_at='2026-04-17T09:00:00+00:00',
    )
    NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=NodeMode.ONLINE)
    conn.commit()

    _enqueue(conn, OperationType.GO_OFFLINE)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ACK', f'GO_OFFLINE failed: {r}'

    session = OfflineRepository.get_open_session(conn, FN)
    assert session is not None, 'Expected an open session after GO_OFFLINE'
    return session.offline_session_id


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def test_go_online_closes_offline_session(conn: sqlite3.Connection) -> None:
    """After GO_ONLINE, the offline session status must be CLOSED and no open session remains."""
    session_id = _go_offline(conn)

    _enqueue(conn, OperationType.GO_ONLINE)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ACK', f'GO_ONLINE failed: {r.canonical_error}'

    # No open session should remain
    open_session = OfflineRepository.get_open_session(conn, FN)
    assert open_session is None, 'Expected no open session after GO_ONLINE'

    # The specific session must be CLOSED in the database
    row = conn.execute(
        "SELECT status FROM offline_sessions WHERE offline_session_id = ?",
        (session_id,),
    ).fetchone()
    assert row is not None, f'Session {session_id} not found in DB'
    assert row[0] == OfflineSessionState.CLOSED.value, (
        f'Expected status={OfflineSessionState.CLOSED.value!r}, got {row[0]!r}'
    )


def test_go_online_sets_ended_at(conn: sqlite3.Connection) -> None:
    """After GO_ONLINE, ended_at must be populated on the offline session record."""
    session_id = _go_offline(conn)

    _enqueue(conn, OperationType.GO_ONLINE)
    _worker().process_next(conn, fiscal_number=FN)

    row = conn.execute(
        "SELECT ended_at FROM offline_sessions WHERE offline_session_id = ?",
        (session_id,),
    ).fetchone()
    assert row is not None, f'Session {session_id} not found in DB'
    assert row[0] is not None, 'ended_at must be set after GO_ONLINE'


def test_go_online_updates_node_month_seconds(conn: sqlite3.Connection) -> None:
    """After GO_ONLINE, current_month_offline_seconds must be >= pre-session value and node is ONLINE."""
    # Capture baseline before entering offline mode
    state_baseline = NodeStateRepository.get_state(conn, FN)
    seconds_baseline = state_baseline.current_month_offline_seconds if state_baseline else 0

    _go_offline(conn)

    state_offline = NodeStateRepository.get_state(conn, FN)
    assert state_offline is not None
    assert state_offline.mode == NodeMode.OFFLINE

    _enqueue(conn, OperationType.GO_ONLINE)
    _worker().process_next(conn, fiscal_number=FN)

    state_after = NodeStateRepository.get_state(conn, FN)
    assert state_after is not None
    assert state_after.mode == NodeMode.ONLINE, (
        f'Expected mode=ONLINE after GO_ONLINE, got {state_after.mode}'
    )
    assert state_after.current_month_offline_seconds >= seconds_baseline, (
        f'Monthly offline seconds must not decrease after GO_ONLINE: '
        f'{state_after.current_month_offline_seconds} < {seconds_baseline}'
    )
