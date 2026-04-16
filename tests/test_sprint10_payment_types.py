"""Sprint 10 step 2: PaymentTypeRepository + guard.

Tests:
  PT1: configured type resolves correctly
  PT2: unknown type returns None from repository
  PT3: inactive type not returned
  PT4: unknown payment type in receipt → rejected before sign
  PT5: CASH always passes without configuration
  PT6: configured type passes through write_path
"""
from __future__ import annotations

from datetime import datetime, UTC

import pytest
from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, PaymentTypeRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


# ---------------------------------------------------------------------------
# PT1 — configured type resolves
# ---------------------------------------------------------------------------

def test_pt1_configured_type_resolves(conn) -> None:
    conn.execute("""
        INSERT INTO payment_type_definitions (fiscal_number, type_code, type_group, name)
        VALUES ('FN-DEV-0001', 'CARD', 2, 'Безготівковий')
    """)
    conn.commit()

    result = PaymentTypeRepository.get_type(conn, 'FN-DEV-0001', 'CARD')
    assert result is not None
    assert result.type_group == 2
    assert result.name == 'Безготівковий'

    all_types = PaymentTypeRepository.get_for_fiscal_number(conn, 'FN-DEV-0001')
    assert 'CARD' in all_types
    assert len(all_types) >= 1


# ---------------------------------------------------------------------------
# PT2 — unknown type returns None
# ---------------------------------------------------------------------------

def test_pt2_unknown_type_returns_none(conn) -> None:
    result = PaymentTypeRepository.get_type(conn, 'FN-DEV-0001', 'CRYPTO_WALLET')
    assert result is None


# ---------------------------------------------------------------------------
# PT3 — inactive type not returned
# ---------------------------------------------------------------------------

def test_pt3_inactive_type_not_returned(conn) -> None:
    conn.execute("""
        INSERT INTO payment_type_definitions (fiscal_number, type_code, type_group, name, is_active)
        VALUES ('FN-DEV-0001', 'OLD_CARD', 2, 'Стара картка', 0)
    """)
    conn.commit()

    result = PaymentTypeRepository.get_type(conn, 'FN-DEV-0001', 'OLD_CARD')
    assert result is None

    all_types = PaymentTypeRepository.get_for_fiscal_number(conn, 'FN-DEV-0001')
    assert 'OLD_CARD' not in all_types


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------

def _setup_shift(conn):
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = 'shift-pt'").fetchone()
    if not existing:
        ShiftRepository.create_shift(
            conn, shift_id='shift-pt', fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-14T08:00:00Z',
        )
        conn.commit()


def _enqueue_sell(conn, rid, payments):
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Item', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': payments,
                'totals': {'total_sum': 1000},
            },
        },
        payload_sha256=f'sha-{rid}',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{rid}'),
        correlation_id=f'c-{rid}',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T12:01:00Z',
    )
    conn.commit()


class _Crypto:
    def __init__(self):
        self.call_count = 0
    def sign(self, **kw):
        self.call_count += 1
        return 'signed'
    def sign_raw(self, *, data, document_id=None):
        self.call_count += 1
        return b'\x30\x82SIGNED'


class _OkTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(state='ACK', transport_request_id='tr',
                          submission_status='DPS_ACK', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


# ---------------------------------------------------------------------------
# PT4 — unknown payment type → rejected before sign
# ---------------------------------------------------------------------------

def test_pt4_unknown_type_rejected(conn) -> None:
    _setup_shift(conn)
    _enqueue_sell(conn, 'req-pt4', [{'amount': 1000, 'type': 'BITCOIN'}])

    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert 'BITCOIN' in result.canonical_error.message
    assert crypto.call_count == 0, 'Crypto must NOT be called for unknown payment type'


# ---------------------------------------------------------------------------
# PT5 — CASH always passes without configuration
# ---------------------------------------------------------------------------

def test_pt5_cash_always_passes(conn) -> None:
    _setup_shift(conn)
    _enqueue_sell(conn, 'req-pt5', [{'amount': 1000, 'type': 'CASH'}])

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ACK', f'CASH must always pass. Got: {result.canonical_error}'


# ---------------------------------------------------------------------------
# PT6 — configured type passes through write_path
# ---------------------------------------------------------------------------

def test_pt6_configured_type_passes(conn) -> None:
    conn.execute("""
        INSERT INTO payment_type_definitions (fiscal_number, type_code, type_group, name)
        VALUES ('FN-DEV-0001', 'VISA', 2, 'VISA')
    """)
    conn.commit()

    _setup_shift(conn)
    _enqueue_sell(conn, 'req-pt6', [{'amount': 1000, 'type': 'VISA'}])

    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ACK', f'Configured VISA must pass. Got: {result.canonical_error}'
    assert crypto.call_count > 0, 'Crypto must be called — guard passed'
