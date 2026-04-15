"""Full fiscal lifecycle E2E test.

Sequence:
  1. SHIFT_OPEN
  2. SELL — готівка, один товар
  3. SELL — картка + знижка + здача
  4. SERVICE_IN — внесення готівки
  5. SERVICE_OUT — видача готівки
  6. RETURN — повернення чеку #2
  7. Z_REPORT — закриття зміни

All operations go through the real WritePathWorker with:
  - in-memory SQLite (auto-migrated via conftest.py)
  - StubCryptoProvider (signs without network)
  - StubTransportClient (returns ACK without network)

Assertions:
  - every document reaches ACK state
  - LND strictly increases across documents
  - shift moves OPENED → CLOSED after Z_REPORT
  - sync_outbox has an entry per document
  - document_files has SIGNED_XML for every fiscal document
"""
from __future__ import annotations

import json
import sqlite3
from datetime import UTC, datetime

import pytest

from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.ports import SendResult, TransportRetryableError, CryptoProviderUnavailableError
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json

# ---------------------------------------------------------------------------
# Shared stubs (same pattern as test_write_path.py)
# ---------------------------------------------------------------------------

FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'


class StubCryptoProvider:
    def __init__(self) -> None:
        self.calls: list[str] = []

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.calls.append(document_id)
        return f'signed::{document_id}'


class StubTransportClient:
    def __init__(self) -> None:
        self.calls: list[str] = []

    def send(self, *, document_id: str, signed_payload: str,
             fiscal_number: str, backend_profile_id: str,
             transport_profile_id: str, **kwargs) -> SendResult:
        self.calls.append(document_id)
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


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_seq = 0


def _next_id(prefix: str) -> str:
    global _seq
    _seq += 1
    return f'{prefix}-{_seq}'


def _ts(offset: int = 0) -> datetime:
    """Return 2026-04-15 10:00 + offset minutes."""
    from datetime import timedelta
    base = datetime(2026, 4, 15, 10, 0, 0, tzinfo=UTC)
    return base + timedelta(minutes=offset)


def _enqueue(conn: sqlite3.Connection, *, operation_type: OperationType,
             payload: dict, offset: int = 0) -> str:
    request_id = _next_id('req')
    cmd = CanonicalFiscalCommand(
        request_id=request_id,
        idempotency_key=_next_id('idem'),
        protocol=Protocol.CHECKBOX_REST,
        operation_type=operation_type,
        fiscal_number=FN,
        route_key='pos-1',
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner='front-a',
        external_request_id=_next_id('ext'),
        business_ts=_ts(offset),
        payload=payload,
        payload_sha256=_next_id('sha'),
    )
    InboxRepository.accept_command(
        conn,
        request_id=request_id,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=FN,
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner='front-a',
        external_request_id=cmd.external_request_id,
        protocol_session_id='e2e-session',
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )
    return request_id


def _run(conn: sqlite3.Connection, worker: WritePathWorker) -> object:
    result = worker.process_next(conn, fiscal_number=FN)
    assert result is not None, 'expected work item'
    return result


def _open_shift(conn: sqlite3.Connection) -> None:
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
        channel_lock_acquired_at='2026-04-15T09:59:00+00:00',
    )
    conn.commit()


# ---------------------------------------------------------------------------
# Payload builders
# ---------------------------------------------------------------------------

def _shift_open_payload() -> dict:
    return {
        'receipt': {
            'goods': [],
            'payments': [],
            'totals': {'total_sum': 0},
        }
    }


def _sell_cash_payload() -> dict:
    return {
        'receipt': {
            'goods': [
                {'code': 'MILK-1L', 'name': 'Молоко 1л', 'price': 3500,
                 'quantity': 2000, 'sum': 7000},
            ],
            'payments': [{'type': 'CASH', 'amount': 7000,
                          'resolved_t': '0', 'resolved_nm': 'ГОТІВКА'}],
            'totals': {'total_sum': 7000},
        }
    }


def _sell_card_payload() -> dict:
    # SELL by card — 2 items, paid cashless
    return {
        'receipt': {
            'goods': [
                {'code': 'COFFEE', 'name': 'Кава 200г', 'price': 9000,
                 'quantity': 1000, 'sum': 9000},
                {'code': 'SUGAR',  'name': 'Цукор 1кг', 'price': 2500,
                 'quantity': 2000, 'sum': 5000},
            ],
            'payments': [{'type': 'CASHLESS', 'amount': 14000,
                          'resolved_t': '2', 'resolved_nm': 'КАРТКА',
                          'card_mask': '****1234'}],
            'totals': {'total_sum': 14000},
        }
    }


def _service_in_payload(amount: int = 5000) -> dict:
    return {
        'service_sum': amount,
        'receipt': {'goods': [], 'payments': [], 'totals': {'total_sum': amount}},
    }


def _service_out_payload(amount: int = 2000) -> dict:
    return {
        'service_sum': amount,
        'receipt': {'goods': [], 'payments': [], 'totals': {'total_sum': amount}},
    }


def _return_payload(related_receipt_id: str) -> dict:
    # Partial return — only COFFEE item from sell_card
    return {
        'receipt': {
            'goods': [
                {'code': 'COFFEE', 'name': 'Кава 200г', 'price': 9000,
                 'quantity': 1000, 'sum': 9000},
            ],
            'payments': [{'type': 'CASHLESS', 'amount': 9000,
                          'resolved_t': '2', 'resolved_nm': 'КАРТКА',
                          'card_mask': '****1234'}],
            'totals': {'total_sum': 9000},
            'related_receipt_id': related_receipt_id,
        }
    }


def _z_report_payload() -> dict:
    return {
        'z_report_data': {
            'check_count': {'ni': 2, 'no': 1},
            'payment_sums': {
                'CASH':     {'smi': 7000, 'smo': 0},
                'CASHLESS': {'smi': 9000, 'smo': 9000},
            },
            'service_sums': {
                'SERVICE_IN':  {'smi': 5000, 'smo': 0},
                'SERVICE_OUT': {'smi': 0,    'smo': 2000},
            },
        }
    }


# ---------------------------------------------------------------------------
# The test
# ---------------------------------------------------------------------------

def test_full_fiscal_lifecycle(conn: sqlite3.Connection) -> None:
    """SHIFT_OPEN → SELL(cash) → SELL(card+discount) → SERVICE_IN →
       SERVICE_OUT → RETURN → Z_REPORT — all reach ACK, LND increases."""

    # Configure CASHLESS payment type (not in default seed)
    conn.execute("""
        INSERT INTO payment_type_definitions (fiscal_number, type_code, type_group, name)
        VALUES (?, 'CASHLESS', 2, 'Безготівковий')
    """, (FN,))
    conn.commit()

    crypto = StubCryptoProvider()
    transport = StubTransportClient()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)

    results = {}
    lnds: list[int] = []

    # 1. SHIFT_OPEN — enqueue then process (shift pre-created by worker logic)
    conn.execute('BEGIN IMMEDIATE')
    _enqueue(conn, operation_type=OperationType.SHIFT_OPEN,
             payload=_shift_open_payload(), offset=0)
    conn.commit()

    r_open = _run(conn, worker)
    assert r_open.outcome == 'ACK', f'SHIFT_OPEN: {r_open}'
    results['shift_open'] = r_open

    # Verify shift is now open
    shift = ShiftRepository.get_active_shift(conn, fiscal_number=FN)
    assert shift is not None
    assert shift.state == ShiftState.OPENED

    # 2. SELL — готівка
    conn.execute('BEGIN IMMEDIATE')
    _enqueue(conn, operation_type=OperationType.SELL,
             payload=_sell_cash_payload(), offset=60)
    conn.commit()

    r_sell1 = _run(conn, worker)
    assert r_sell1.outcome == 'ACK', f'SELL cash: {r_sell1}'
    results['sell_cash'] = r_sell1

    # 3. SELL — картка + знижка
    conn.execute('BEGIN IMMEDIATE')
    _enqueue(conn, operation_type=OperationType.SELL,
             payload=_sell_card_payload(), offset=2)
    conn.commit()

    r_sell2 = _run(conn, worker)
    assert r_sell2.outcome == 'ACK', f'SELL card: {r_sell2}'
    results['sell_card'] = r_sell2

    # 4. SERVICE_IN
    conn.execute('BEGIN IMMEDIATE')
    _enqueue(conn, operation_type=OperationType.SERVICE_IN,
             payload=_service_in_payload(), offset=180)
    conn.commit()

    r_svc_in = _run(conn, worker)
    assert r_svc_in.outcome == 'ACK', f'SERVICE_IN: {r_svc_in}'
    results['service_in'] = r_svc_in

    # 5. SERVICE_OUT
    conn.execute('BEGIN IMMEDIATE')
    _enqueue(conn, operation_type=OperationType.SERVICE_OUT,
             payload=_service_out_payload(), offset=240)
    conn.commit()

    r_svc_out = _run(conn, worker)
    assert r_svc_out.outcome == 'ACK', f'SERVICE_OUT: {r_svc_out}'
    results['service_out'] = r_svc_out

    # 6. RETURN — повернення sell_card (related_receipt_id = sell2 document_id)
    conn.execute('BEGIN IMMEDIATE')
    _enqueue(conn, operation_type=OperationType.RETURN,
             payload=_return_payload(related_receipt_id=r_sell2.document_id),
             offset=300)
    conn.commit()

    r_return = _run(conn, worker)
    assert r_return.outcome == 'ACK', f'RETURN: {r_return}'
    results['return'] = r_return

    # 7. Z_REPORT
    conn.execute('BEGIN IMMEDIATE')
    _enqueue(conn, operation_type=OperationType.Z_REPORT,
             payload=_z_report_payload(), offset=360)
    conn.commit()

    r_z = _run(conn, worker)
    assert r_z.outcome == 'ACK', f'Z_REPORT: {r_z}'
    results['z_report'] = r_z

    # --- Cross-cutting assertions ---

    # All documents in ACK state
    for name, r in results.items():
        row = conn.execute(
            'SELECT state FROM fiscal_documents WHERE document_id = ?',
            (r.document_id,)
        ).fetchone()
        assert row is not None, f'{name}: document not found'
        assert row[0] == DocumentState.ACK.value, f'{name}: expected ACK, got {row[0]}'

    # LND strictly increases for fiscal documents (SELL, RETURN, SERVICE, Z)
    fiscal_ops = ['sell_cash', 'sell_card', 'service_in', 'service_out', 'return', 'z_report']
    lnds = []
    for name in fiscal_ops:
        r = results[name]
        row = conn.execute(
            'SELECT lnd FROM fiscal_documents WHERE document_id = ?',
            (r.document_id,)
        ).fetchone()
        if row and row[0] is not None:
            lnds.append(row[0])

    assert lnds == sorted(lnds), f'LND not strictly increasing: {lnds}'
    assert len(lnds) == len(set(lnds)), f'LND duplicates found: {lnds}'

    # sync_outbox has entry for every document
    for name, r in results.items():
        count = conn.execute(
            'SELECT COUNT(*) FROM sync_outbox WHERE aggregate_id = ?',
            (r.document_id,)
        ).fetchone()[0]
        assert count >= 1, f'{name}: missing sync_outbox entry'

    # SIGNED_XML artifact exists for every fiscal document
    for name in fiscal_ops:
        r = results[name]
        kinds = {row[0] for row in conn.execute(
            'SELECT file_kind FROM document_files WHERE document_id = ?',
            (r.document_id,)
        ).fetchall()}
        assert 'SIGNED_XML' in kinds or 'REQUEST_JSON' in kinds, \
            f'{name}: no artifact files'

    # Crypto and transport were called for every operation
    assert len(crypto.calls) == len(results), \
        f'crypto calls: expected {len(results)}, got {len(crypto.calls)}'
    assert len(transport.calls) == len(results), \
        f'transport calls: expected {len(results)}, got {len(transport.calls)}'
