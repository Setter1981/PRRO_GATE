"""Sprint 8 step 6: RETURN linkage production gate tests.

Tests cover:
  RG1: RETURN without related_receipt_id rejected when require_return_linkage=True
  RG2: RETURN with related_receipt_id passes the gate
  RG3: dev mode (require_return_linkage=False) allows RETURN without linkage
  RG4: SELL unaffected by the gate
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


def _setup_shift_and_enqueue(conn, *, rid: str, op: OperationType, payload: dict, shift_id: str = 'shift-rg'):
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = ?", (shift_id,)).fetchone()
    if not existing:
        ShiftRepository.create_shift(
            conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-13T12:00:00Z',
        )
        conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.CHECKBOX_REST, operation_type=op,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload=payload,
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
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()


class _Crypto:
    def sign(self, **kw): return 'signed'
    def sign_raw(self, *, data, document_id=None): return b'\x30\x82SIGNED'


class _OkTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(state='ACK', transport_request_id='tr',
                          submission_status='DPS_ACK', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


_RETURN_PAYLOAD_NO_LINKAGE = {
    'receipt': {
        'type': 'RETURN',
        'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
        'payments': [{'amount': 100, 'type': 'CASH'}],
        'totals': {'total_sum': 100},
    },
}

_RETURN_PAYLOAD_WITH_LINKAGE = {
    'receipt': {
        'type': 'RETURN',
        'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
        'payments': [{'amount': 100, 'type': 'CASH'}],
        'totals': {'total_sum': 100},
        'related_receipt_id': 'original-receipt-123',
    },
}

_SELL_PAYLOAD = {
    'receipt': {
        'type': 'SELL',
        'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
        'payments': [{'amount': 100, 'type': 'CASH'}],
        'totals': {'total_sum': 100},
    },
}


# ---------------------------------------------------------------------------
# RG1 — RETURN without linkage rejected in production mode
# ---------------------------------------------------------------------------

def test_rg1_return_no_linkage_rejected_production(conn) -> None:
    _setup_shift_and_enqueue(conn, rid='req-rg1', op=OperationType.RETURN,
                             payload=_RETURN_PAYLOAD_NO_LINKAGE)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(),
                             tax_number='TN', require_return_linkage=True)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ERROR'
    assert result.canonical_error is not None
    assert result.canonical_error.code == CanonicalErrorCode.RETURN_LINKAGE_REQUIRED


# ---------------------------------------------------------------------------
# RG2 — RETURN with linkage passes the gate
# ---------------------------------------------------------------------------

def test_rg2_return_with_linkage_passes(conn) -> None:
    _setup_shift_and_enqueue(conn, rid='req-rg2', op=OperationType.RETURN,
                             payload=_RETURN_PAYLOAD_WITH_LINKAGE)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(),
                             tax_number='TN', require_return_linkage=True)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ACK', f'RETURN with linkage should ACK, got {result.outcome}: {result.canonical_error}'


# ---------------------------------------------------------------------------
# RG3 — dev mode allows RETURN without linkage
# ---------------------------------------------------------------------------

def test_rg3_dev_mode_allows_no_linkage(conn) -> None:
    _setup_shift_and_enqueue(conn, rid='req-rg3', op=OperationType.RETURN,
                             payload=_RETURN_PAYLOAD_NO_LINKAGE)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(),
                             tax_number='TN', require_return_linkage=False)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ACK', f'Dev mode RETURN without linkage should ACK, got {result.outcome}'


# ---------------------------------------------------------------------------
# RG4 — SELL unaffected by the gate
# ---------------------------------------------------------------------------

def test_rg4_sell_unaffected(conn) -> None:
    _setup_shift_and_enqueue(conn, rid='req-rg4', op=OperationType.SELL,
                             payload=_SELL_PAYLOAD)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(),
                             tax_number='TN', require_return_linkage=True)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ACK', f'SELL should be unaffected by RETURN gate, got {result.outcome}'
