"""Sprint 8 step 5: DPS RETURN support tests.

Tests cover:
  RT1: XML serializer emits <C T="1"> for RETURN
  RT2: RETURN XML has same goods/payments/total structure as SELL
  RT3: transport accepts RETURN (no longer rejected)
  RT4: transport maps RETURN to check_type=CHK(1)
  RT5: write-path signs DPS XML for RETURN profile
"""
from __future__ import annotations

import pytest
from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


# ---------------------------------------------------------------------------
# RT1 — XML serializer emits <C T="1"> for RETURN
# ---------------------------------------------------------------------------

def test_rt1_return_xml_type() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.RETURN,
        fiscal_number='FN-001',
        local_number=3,
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'RETURN',
                'goods': [{'name': 'Item', 'price': 500, 'quantity': 1000, 'sum': 500}],
                'payments': [{'amount': 500, 'type': 'CASH'}],
                'totals': {'total_sum': 500},
            },
        },
        tax_number='TN-001',
        previous_hash='abc123',
    )
    assert '<C T="1">' in xml, f'RETURN must use <C T="1">, got: {xml}'
    assert '<C T="0">' not in xml, 'RETURN must NOT contain SELL type T="0"'


# ---------------------------------------------------------------------------
# RT2 — RETURN XML has same goods/payments/total structure as SELL
# ---------------------------------------------------------------------------

def test_rt2_return_xml_has_goods_payments() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.RETURN,
        fiscal_number='FN-002',
        local_number=4,
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'RETURN',
                'goods': [
                    {'name': 'Widget', 'price': 1000, 'quantity': 2000, 'sum': 2000},
                ],
                'payments': [
                    {'amount': 2000, 'type': 'CASHLESS'},
                ],
                'totals': {'total_sum': 2000},
            },
        },
        tax_number='TN-002',
    )
    # Goods item
    assert 'NM="Widget"' in xml
    assert 'SM="2000"' in xml
    assert 'Q="2000"' in xml
    # Payment — cashless = M T="2"
    assert '<M ' in xml
    assert 'T="2"' in xml
    # Total
    assert '<E ' in xml


# ---------------------------------------------------------------------------
# RT3 — transport accepts RETURN
# ---------------------------------------------------------------------------

def test_rt3_transport_accepts_return() -> None:
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    class _MockResp:
        id = 'RET-001'
        status = 1
        error_message = ''

    class _MockStub:
        def sendChkV2(self, req, *, timeout=None):
            return _MockResp()

    transport = DpsFiscalServerTransport(grpc_stub=_MockStub())
    result = transport.send(
        document_id='doc-ret-1',
        signed_payload=b'\x30\x82test',
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='RETURN',
    )
    assert result.state == 'ACK'
    assert result.server_fiscal_no == 'RET-001'


# ---------------------------------------------------------------------------
# RT4 — transport maps RETURN to check_type=CHK(1)
# ---------------------------------------------------------------------------

def test_rt4_return_check_type_is_chk() -> None:
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    captured = {}

    class _CapStub:
        def sendChkV2(self, req, *, timeout=None):
            captured['check_type'] = req.check_type
            captured['local_number'] = req.local_number

            class _R:
                id = 'X'
                status = 1
                error_message = ''
            return _R()

    transport = DpsFiscalServerTransport(grpc_stub=_CapStub())
    transport.send(
        document_id='doc-ret-2',
        signed_payload=b'\x30\x82test',
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='RETURN',
        lnd=5,
    )
    assert captured['check_type'] == 1, f'RETURN check_type must be CHK(1), got {captured["check_type"]}'
    assert captured['local_number'] == 5, f'RETURN local_number must use lnd, got {captured["local_number"]}'


# ---------------------------------------------------------------------------
# RT6 — transport passes related_receipt_id as id_cancel on proto
# ---------------------------------------------------------------------------

def test_rt6_return_id_cancel_from_related_receipt() -> None:
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    captured = {}

    class _CapStub:
        def sendChkV2(self, req, *, timeout=None):
            captured['id_cancel'] = req.id_cancel

            class _R:
                id = 'X'
                status = 1
                error_message = ''
            return _R()

    transport = DpsFiscalServerTransport(grpc_stub=_CapStub())
    transport.send(
        document_id='doc-rt6',
        signed_payload=b'\x30\x82test',
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='RETURN',
        lnd=3,
        related_receipt_id='ORIG-FISCAL-ID-123',
    )
    assert captured['id_cancel'] == 'ORIG-FISCAL-ID-123', (
        f'RETURN id_cancel must carry related_receipt_id, got {captured["id_cancel"]!r}'
    )


def test_rt7_sell_id_cancel_empty() -> None:
    """SELL should send empty id_cancel."""
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    captured = {}

    class _CapStub:
        def sendChkV2(self, req, *, timeout=None):
            captured['id_cancel'] = req.id_cancel

            class _R:
                id = 'X'
                status = 1
                error_message = ''
            return _R()

    transport = DpsFiscalServerTransport(grpc_stub=_CapStub())
    transport.send(
        document_id='doc-rt7',
        signed_payload=b'\x30\x82test',
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='SELL',
    )
    assert captured['id_cancel'] == '', f'SELL id_cancel must be empty, got {captured["id_cancel"]!r}'


# ---------------------------------------------------------------------------
# RT5 — write-path signs DPS XML for RETURN profile
# ---------------------------------------------------------------------------

def test_rt5_write_path_return_dps_xml(conn) -> None:
    """Write-path must build DPS XML with T="1" for RETURN on DPS profile."""
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-rt5', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-rt5', idempotency_key='idem-rt5',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.RETURN,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-rt5',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'RETURN',
                'goods': [{'name': 'Refund item', 'price': 300, 'quantity': 1000, 'sum': 300}],
                'payments': [{'amount': 300, 'type': 'CASH'}],
                'totals': {'total_sum': 300},
                'related_receipt_id': 'orig-receipt-001',
            },
        },
        payload_sha256='sha-rt5',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-rt5'),
        correlation_id='c-rt5',
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

    signed_inputs = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_inputs.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-rt5',
                              submission_status='DPS_ACK', server_fiscal_no='SFN-RT5',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport(), tax_number='TN-RT5')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert len(signed_inputs) == 1
    xml = signed_inputs[0]
    assert '<C T="1">' in xml, f'RETURN write-path XML must use T="1", got: {xml}'
    assert 'NM="Refund item"' in xml
