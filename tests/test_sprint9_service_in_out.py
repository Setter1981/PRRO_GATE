"""Sprint 9 step 1: SERVICE_IN and SERVICE_OUT DPS support tests.

Tests cover:
  SV1: SERVICE_IN XML shape — <C T="2"><I N="1" T="0" .../>
  SV2: SERVICE_OUT XML shape — <C T="2"><O N="1" T="0" .../>
  SV3: service XML has no <P> or <M> elements
  SV4: transport accepts SERVICE_IN, maps to SERVICECHK(3)
  SV5: transport accepts SERVICE_OUT, maps to SERVICECHK(3)
  SV6: write-path signs DPS XML for SERVICE_IN on DPS profile
  SV7: write-path signs DPS XML for SERVICE_OUT on DPS profile
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


# ---------------------------------------------------------------------------
# SV1 — SERVICE_IN XML shape
# ---------------------------------------------------------------------------

def test_sv1_service_in_xml_shape() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SERVICE_IN,
        fiscal_number='FN-001',
        local_number=5,
        business_ts=datetime(2026, 4, 14, 8, 0, 0, tzinfo=UTC),
        payload={'service_sum': 500000},
        tax_number='TN-001',
        previous_hash='abc123',
    )
    assert '<C T="2">' in xml
    assert '<I N="1" NM="ГОТІВКА" SM="500000" T="0"></I>' in xml
    assert '<E N="2"></E>' in xml
    assert 'DI="5"' in xml


# ---------------------------------------------------------------------------
# SV2 — SERVICE_OUT XML shape
# ---------------------------------------------------------------------------

def test_sv2_service_out_xml_shape() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SERVICE_OUT,
        fiscal_number='FN-001',
        local_number=6,
        business_ts=datetime(2026, 4, 14, 18, 0, 0, tzinfo=UTC),
        payload={'service_sum': 300000},
        tax_number='TN-001',
        previous_hash='def456',
    )
    assert '<C T="2">' in xml
    assert '<O N="1" NM="ГОТІВКА" SM="300000" T="0"></O>' in xml
    assert '<E N="2"></E>' in xml


# ---------------------------------------------------------------------------
# SV3 — no <P> or <M> in service checks
# ---------------------------------------------------------------------------

def test_sv3_service_no_goods_payments() -> None:
    for op in (OperationType.SERVICE_IN, OperationType.SERVICE_OUT):
        xml = build_dps_xml(
            operation_type=op,
            fiscal_number='FN-001',
            local_number=1,
            business_ts=datetime(2026, 4, 14, 8, 0, 0, tzinfo=UTC),
            payload={'service_sum': 100},
            tax_number='TN-001',
        )
        assert '<P ' not in xml, f'{op.value} must not have <P> goods elements'
        assert '<M ' not in xml, f'{op.value} must not have <M> payment elements'


# ---------------------------------------------------------------------------
# SV4 — transport accepts SERVICE_IN, check_type=SERVICECHK(3)
# ---------------------------------------------------------------------------

def test_sv4_transport_service_in_servicechk() -> None:
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    captured = {}

    class _CapStub:
        def sendChkV2(self, req):
            captured['check_type'] = req.check_type

            class _R:
                id = 'SVC-IN-001'
                status = 1
                error_message = ''
            return _R()

    transport = DpsFiscalServerTransport(grpc_stub=_CapStub())
    result = transport.send(
        document_id='doc-sv4',
        signed_payload=b'\x30\x82test',
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='SERVICE_IN',
        lnd=5,
    )
    assert result.state == 'ACK'
    assert captured['check_type'] == 1, f'SERVICE_IN check_type must be CHK(1), got {captured["check_type"]}'


# ---------------------------------------------------------------------------
# SV5 — transport accepts SERVICE_OUT, check_type=SERVICECHK(3)
# ---------------------------------------------------------------------------

def test_sv5_transport_service_out_servicechk() -> None:
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    captured = {}

    class _CapStub:
        def sendChkV2(self, req):
            captured['check_type'] = req.check_type

            class _R:
                id = 'SVC-OUT-001'
                status = 1
                error_message = ''
            return _R()

    transport = DpsFiscalServerTransport(grpc_stub=_CapStub())
    result = transport.send(
        document_id='doc-sv5',
        signed_payload=b'\x30\x82test',
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='SERVICE_OUT',
        lnd=6,
    )
    assert result.state == 'ACK'
    assert captured['check_type'] == 1, f'SERVICE_OUT check_type must be CHK(1), got {captured["check_type"]}'


# ---------------------------------------------------------------------------
# SV6 — write-path SERVICE_IN through DPS profile
# ---------------------------------------------------------------------------

def test_sv6_write_path_service_in_dps_xml(conn) -> None:
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-sv6', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-sv6', idempotency_key='idem-sv6',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SERVICE_IN,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-sv6',
        business_ts=datetime(2026, 4, 14, 8, 0, 0, tzinfo=UTC),
        payload={'service_sum': 500000, 'receipt': {'type': 'SERVICE_IN', 'goods': [], 'payments': [], 'totals': {}}},
        payload_sha256='sha-sv6',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-sv6'),
        correlation_id='c-sv6',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T08:01:00Z',
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
            return SendResult(state='ACK', transport_request_id='tr-sv6',
                              submission_status='DPS_ACK', server_fiscal_no='SFN-SV6',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport(), tax_number='TN-SV6')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert len(signed_inputs) == 1
    xml = signed_inputs[0]
    assert '<C T="2">' in xml
    assert '<I N="1" NM="ГОТІВКА" SM="500000" T="0"></I>' in xml


# ---------------------------------------------------------------------------
# SV7 — write-path SERVICE_OUT through DPS profile
# ---------------------------------------------------------------------------

def test_sv7_write_path_service_out_dps_xml(conn) -> None:
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    import json as _json
    ShiftRepository.create_shift(
        conn, shift_id='shift-sv7', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    # Seed cash balance so SERVICE_OUT guard passes
    _pj = _json.dumps({'service_sum': 999999})
    conn.execute("INSERT INTO ingress_inbox (request_id, idempotency_key, protocol, operation_type, fiscal_number, backend_profile_id, transport_profile_id, channel_owner, payload_json, payload_sha256, status, response_deadline_at) VALUES ('req-sv7-seed', 'idem-sv7-seed', 'CHECKBOX_REST', 'SERVICE_IN', 'FN-DEV-0001', 'backend_checkbox_default', 'transport_dps_grpc_default', 'test', ?, 'sha', 'DONE', '2026-04-14T23:00:00Z')", (_pj,))
    conn.execute("INSERT INTO fiscal_documents (document_id, request_id, fiscal_number, doc_type, state, fs_mode, lnd, backend_profile_id, transport_profile_id, submission_status, payload_json, payload_sha256, business_ts, shift_id) VALUES ('doc-sv7-seed', 'req-sv7-seed', 'FN-DEV-0001', 'SERVICE_IN', 'ACK', 'ONLINE', 500, 'backend_checkbox_default', 'transport_dps_grpc_default', 'DPS_ACK', ?, 'sha', '2026-04-14T08:00:00Z', 'shift-sv7')", (_pj,))
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-sv7', idempotency_key='idem-sv7',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SERVICE_OUT,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-sv7',
        business_ts=datetime(2026, 4, 14, 18, 0, 0, tzinfo=UTC),
        payload={'service_sum': 300000, 'receipt': {'type': 'SERVICE_OUT', 'goods': [], 'payments': [], 'totals': {}}},
        payload_sha256='sha-sv7',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-sv7'),
        correlation_id='c-sv7',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T18:01:00Z',
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
            return SendResult(state='ACK', transport_request_id='tr-sv7',
                              submission_status='DPS_ACK', server_fiscal_no='SFN-SV7',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport(), tax_number='TN-SV7')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert len(signed_inputs) == 1
    xml = signed_inputs[0]
    assert '<C T="2">' in xml
    assert '<O N="1" NM="ГОТІВКА" SM="300000" T="0"></O>' in xml
