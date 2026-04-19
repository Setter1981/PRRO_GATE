"""Sprint 11 — Full offline lifecycle E2E (step 11.4).

Sequence under test:
  E2E-1  ASK_OFFLINE_CODES              → range [1001–1050] ACTIVE
  E2E-2  Shift opened                   → OPENED (created directly, not via SHIFT_OPEN fiscal cmd)
  E2E-3  GO_OFFLINE                     → node OFFLINE, offline_session OPEN
  E2E-4  SELL #1 (offline)              → OFFLINE_LOCAL_ACK, offline_fiscal_no=1001
  E2E-5  SELL #2 (offline)              → OFFLINE_LOCAL_ACK, offline_fiscal_no=1002
  E2E-6  GO_ONLINE attempt              → ERROR OFFLINE_BACKLOG_NOT_SYNCED
  E2E-7  OfflineSyncService.sync_pending→ synced=2, both SELLs now ACK
  E2E-8  GO_ONLINE                      → node ONLINE, session CLOSED
  E2E-9  Z_REPORT (online)              → ACK

Cross-cutting assertions:
  - offline_fiscal_no values are consecutive from range (1001, 1002)
  - range next_fiscal_no advanced to 1003
  - LND strictly monotonic across all fiscal docs (SELLs + Z_REPORT)
  - offline_sessions row closed with ended_at populated
  - no open session after GO_ONLINE
"""
from __future__ import annotations

import json
import sqlite3
from datetime import UTC, datetime

import pytest

from prro_gateway.enums import DocumentState, NodeMode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.services.offline_sync import OfflineSyncService
from prro_gateway.utils.json_codec import dumps_json

FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'

_seq = 0


def _nid(p: str) -> str:
    global _seq
    _seq += 1
    return f'{p}-e2e-{_seq}'


class _StubCrypto:
    def sign(self, *, document_id: str, payload_json: str) -> str:
        return f'sig::{document_id}'


class _StubTransport:
    """Stub: always returns ACK synchronously."""

    def send(self, **kw):
        from prro_gateway.ports import SendResult
        now = datetime.now(UTC)
        return SendResult(
            state='ACK',
            transport_request_id='tx-' + kw['document_id'],
            submission_status='ACK',
            server_fiscal_no='SFN-' + kw['document_id'],
            server_fiscal_date=now.isoformat(),
            response_json='{}',
            sent_at=now,
            ack_at=now,
        )


def _worker() -> WritePathWorker:
    return WritePathWorker(
        crypto_provider=_StubCrypto(),
        transport_client=_StubTransport(),
    )


def _sync_svc() -> OfflineSyncService:
    return OfflineSyncService(transport_client=_StubTransport())


def _sell_payload() -> dict:
    return {'receipt': {
        'goods': [{'code': '1', 'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
        'payments': [{'type': 'CASH', 'amount': 1000, 'resolved_t': '0', 'resolved_nm': 'Готівка'}],
        'totals': {'total_sum': 1000},
    }}


def _enqueue(conn: sqlite3.Connection, op: OperationType, payload: dict | None = None) -> str:
    rid = _nid('req')
    cmd = CanonicalFiscalCommand(
        request_id=rid,
        idempotency_key=_nid('idem'),
        protocol=Protocol.CHECKBOX_REST,
        operation_type=op,
        fiscal_number=FN,
        route_key='pos-1',
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner='front-a',
        external_request_id=_nid('ext'),
        business_ts=datetime(2026, 4, 15, 10, 0, 0, tzinfo=UTC),
        payload=payload if payload is not None else {},
        payload_sha256=_nid('sha'),
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


# ---------------------------------------------------------------------------
# Single E2E scenario: full offline lifecycle from codes to Z_REPORT
# ---------------------------------------------------------------------------

def test_e2e_full_offline_lifecycle(conn: sqlite3.Connection) -> None:
    worker = _worker()

    # -----------------------------------------------------------------------
    # E2E-1: Register offline code range
    # -----------------------------------------------------------------------
    _enqueue(conn, OperationType.ASK_OFFLINE_CODES, {'first_fiscal_no': 1001, 'last_fiscal_no': 1050})
    r_ask = worker.process_next(conn, fiscal_number=FN)
    assert r_ask.outcome == 'ACK', f'E2E-1 failed: {r_ask}'

    rng = OfflineRepository.get_active_range(conn, FN)
    assert rng is not None
    assert rng.first_fiscal_no == 1001
    assert rng.last_fiscal_no == 1050
    assert rng.next_fiscal_no == 1001
    assert rng.status == 'ACTIVE'

    # -----------------------------------------------------------------------
    # E2E-2: Open shift (direct DB, not via SHIFT_OPEN fiscal command)
    # -----------------------------------------------------------------------
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn,
        shift_id='shift-e2e',
        fiscal_number=FN,
        state=ShiftState.OPENED,
        open_mode='ONLINE',
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST,
        integration_owner='front-a',
        channel_lock_acquired_at='2026-04-15T10:00:00+00:00',
    )
    conn.commit()

    # -----------------------------------------------------------------------
    # E2E-3: GO_OFFLINE
    # -----------------------------------------------------------------------
    _enqueue(conn, OperationType.GO_OFFLINE)
    r_offline = worker.process_next(conn, fiscal_number=FN)
    assert r_offline.outcome == 'ACK', f'E2E-3 GO_OFFLINE failed: {r_offline}'

    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.OFFLINE

    session = OfflineRepository.get_open_session(conn, FN)
    assert session is not None
    session_id = session.offline_session_id

    # -----------------------------------------------------------------------
    # E2E-4: SELL #1 (offline) — should get offline_fiscal_no=1001
    # -----------------------------------------------------------------------
    _enqueue(conn, OperationType.SELL, _sell_payload())
    r_sell1 = worker.process_next(conn, fiscal_number=FN)
    assert r_sell1.outcome == 'OFFLINE_LOCAL_ACK', f'E2E-4 SELL#1 failed: {r_sell1}'

    row1 = conn.execute(
        "SELECT lnd, offline_fiscal_no, state FROM fiscal_documents WHERE document_id = ?",
        (r_sell1.document_id,),
    ).fetchone()
    assert row1 is not None
    lnd_sell1 = row1[0]
    assert row1[1] == 1001, f'offline_fiscal_no expected 1001, got {row1[1]}'
    assert row1[2] == DocumentState.OFFLINE_LOCAL_ACK.value

    # -----------------------------------------------------------------------
    # E2E-5: SELL #2 (offline) — should get offline_fiscal_no=1002
    # -----------------------------------------------------------------------
    _enqueue(conn, OperationType.SELL, _sell_payload())
    r_sell2 = worker.process_next(conn, fiscal_number=FN)
    assert r_sell2.outcome == 'OFFLINE_LOCAL_ACK', f'E2E-5 SELL#2 failed: {r_sell2}'

    row2 = conn.execute(
        "SELECT lnd, offline_fiscal_no, state FROM fiscal_documents WHERE document_id = ?",
        (r_sell2.document_id,),
    ).fetchone()
    assert row2 is not None
    lnd_sell2 = row2[0]
    assert row2[1] == 1002, f'offline_fiscal_no expected 1002, got {row2[1]}'
    assert row2[2] == DocumentState.OFFLINE_LOCAL_ACK.value

    # range pointer must have advanced by 2
    rng = OfflineRepository.get_active_range(conn, FN)
    assert rng.next_fiscal_no == 1003, f'next_fiscal_no expected 1003, got {rng.next_fiscal_no}'

    # -----------------------------------------------------------------------
    # E2E-6: GO_ONLINE attempt — must be blocked by backlog
    # -----------------------------------------------------------------------
    _enqueue(conn, OperationType.GO_ONLINE)
    r_early_online = worker.process_next(conn, fiscal_number=FN)
    assert r_early_online.outcome == 'ERROR', f'E2E-6 expected ERROR, got: {r_early_online}'
    assert r_early_online.canonical_error.code == 'OFFLINE_BACKLOG_NOT_SYNCED', (
        f'E2E-6 wrong error: {r_early_online.canonical_error.code}'
    )

    # Node must still be OFFLINE
    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.OFFLINE

    # -----------------------------------------------------------------------
    # E2E-7: Sync pending offline docs
    # -----------------------------------------------------------------------
    sync_result = _sync_svc().sync_pending(conn, fiscal_number=FN)
    assert sync_result.synced == 2, f'E2E-7 expected synced=2, got: {sync_result}'
    assert sync_result.checked == 2
    assert sync_result.retryable == 0
    assert sync_result.manual == 0

    # Both SELL docs must now be ACK
    sell1_state = conn.execute(
        "SELECT state FROM fiscal_documents WHERE document_id = ?",
        (r_sell1.document_id,),
    ).fetchone()[0]
    sell2_state = conn.execute(
        "SELECT state FROM fiscal_documents WHERE document_id = ?",
        (r_sell2.document_id,),
    ).fetchone()[0]
    assert sell1_state == DocumentState.ACK.value
    assert sell2_state == DocumentState.ACK.value

    # -----------------------------------------------------------------------
    # E2E-8: GO_ONLINE — must succeed now
    # -----------------------------------------------------------------------
    _enqueue(conn, OperationType.GO_ONLINE)
    r_online = worker.process_next(conn, fiscal_number=FN)
    assert r_online.outcome == 'ACK', f'E2E-8 GO_ONLINE failed: {r_online}'

    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.ONLINE

    # Session must be CLOSED with ended_at populated
    sess_row = conn.execute(
        "SELECT status, ended_at FROM offline_sessions WHERE offline_session_id = ?",
        (session_id,),
    ).fetchone()
    assert sess_row is not None
    assert sess_row[0] == 'CLOSED', f'session status expected CLOSED, got {sess_row[0]}'
    assert sess_row[1] is not None, 'ended_at must be populated after GO_ONLINE'

    # No open session
    assert OfflineRepository.get_open_session(conn, FN) is None

    # -----------------------------------------------------------------------
    # E2E-9: Z_REPORT (online) — must succeed
    # -----------------------------------------------------------------------
    _enqueue(conn, OperationType.Z_REPORT, {})
    r_zrep = worker.process_next(conn, fiscal_number=FN)
    assert r_zrep.outcome == 'ACK', f'E2E-9 Z_REPORT failed: {r_zrep}'

    lnd_zrep = conn.execute(
        "SELECT lnd FROM fiscal_documents WHERE document_id = ?",
        (r_zrep.document_id,),
    ).fetchone()[0]

    # -----------------------------------------------------------------------
    # Cross-cutting: LND strictly monotonic across all fiscal docs
    # -----------------------------------------------------------------------
    assert lnd_sell1 < lnd_sell2 < lnd_zrep, (
        f'LND not monotonic: sell1={lnd_sell1}, sell2={lnd_sell2}, z_report={lnd_zrep}'
    )

    # All offline_fiscal_no values from range are distinct
    offline_codes = conn.execute(
        "SELECT offline_fiscal_no FROM fiscal_documents WHERE fiscal_number = ? AND offline_fiscal_no IS NOT NULL ORDER BY offline_fiscal_no",
        (FN,),
    ).fetchall()
    codes = [row[0] for row in offline_codes]
    assert codes == [1001, 1002], f'Expected offline_fiscal_no=[1001,1002], got {codes}'
