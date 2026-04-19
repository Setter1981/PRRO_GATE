"""Sprint 9 step 7: CASH_WITHDRAWAL (T=8) support.

Tests cover:
  CW1: XML shape — <C T="8"> with P (ГРИВНА) + M (card details) + E
  CW2: transport accepts CASH_WITHDRAWAL, check_type=CHK(1)
  CW3: XML contains card payment details (PA, PD, RRN)
  CW4: full pipeline — adapter → write_path → serializer → signed XML
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


# ---------------------------------------------------------------------------
# CW1 — XML shape
# ---------------------------------------------------------------------------

def test_cw1_xml_shape() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.CASH_WITHDRAWAL,
        fiscal_number='FN-001', local_number=5,
        business_ts=datetime(2026, 4, 14, 14, 0, 0, tzinfo=UTC),
        payload={
            'cash_withdrawal_sum': 10000,
            'receipt': {
                'type': 'CASH_WITHDRAWAL',
                'goods': [],
                'payments': [{'amount': 10000, 'type': 'CASH',
                              'bank_name': 'ПриватБанк', 'card_mask': 'VISA ****1234',
                              'auth_code': '390546', 'rrn': '044702496008',
                              'payment_system': 'VISA', 'commission': 200}],
                'totals': {'total_sum': 10000},
            },
        },
        tax_number='TN',
    )
    assert '<C T="8">' in xml, f'Must be T=8. Got: {xml}'
    assert 'NM="Гривня"' in xml, f'P must have NM=ГРИВНА. Got: {xml}'
    assert 'SM="10000"' in xml
    assert '<E ' in xml
    assert 'FN="FN-001"' in xml


# ---------------------------------------------------------------------------
# CW2 — transport accepts, check_type=CHK(1)
# ---------------------------------------------------------------------------

def test_cw2_transport_accepts() -> None:
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    captured = {}

    class _CapStub:
        def sendChkV2(self, req, *, timeout=None):
            captured['check_type'] = req.check_type

            class _R:
                id = 'CW-001'
                status = 1
                error_message = ''
            return _R()

    transport = DpsFiscalServerTransport(grpc_stub=_CapStub())
    result = transport.send(
        document_id='doc-cw2', signed_payload=b'\x30\x82test',
        fiscal_number='FN-001', backend_profile_id='bp',
        transport_profile_id='tp', operation_type='CASH_WITHDRAWAL', lnd=5,
    )
    assert result.state == 'ACK'
    assert captured['check_type'] == 1, f'CASH_WITHDRAWAL check_type must be CHK(1), got {captured["check_type"]}'


# ---------------------------------------------------------------------------
# CW3 — card payment details in XML
# ---------------------------------------------------------------------------

def test_cw3_card_details_in_xml() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.CASH_WITHDRAWAL,
        fiscal_number='FN-001', local_number=5,
        business_ts=datetime(2026, 4, 14, 14, 0, 0, tzinfo=UTC),
        payload={
            'cash_withdrawal_sum': 50000,
            'receipt': {
                'type': 'CASH_WITHDRAWAL',
                'goods': [],
                'payments': [{'amount': 50000, 'type': 'CASH',
                              'bank_name': 'ПриватБанк', 'card_mask': 'MASTER ****8482',
                              'auth_code': '390546', 'rrn': '044702496008',
                              'payment_system': 'MasterCard', 'terminal': 'S1KI0NLY',
                              'commission': 500}],
                'totals': {},
            },
        },
        tax_number='TN',
    )
    assert 'PA="ПриватБанк"' in xml, f'Bank name missing. Got: {xml}'
    assert 'PD="MASTER ****8482"' in xml, f'Card mask missing. Got: {xml}'
    assert 'RRN="044702496008"' in xml, f'RRN missing. Got: {xml}'
    assert 'PE="390546"' in xml, f'Auth code missing. Got: {xml}'
    assert 'PF="500"' in xml, f'Commission missing. Got: {xml}'
    assert 'PSNM="MasterCard"' in xml, f'Payment system missing. Got: {xml}'
    assert 'PB="S1KI0NLY"' in xml, f'Terminal missing. Got: {xml}'


# ---------------------------------------------------------------------------
# CW4 — full pipeline
# ---------------------------------------------------------------------------

def test_cw4_full_pipeline(conn) -> None:
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    ShiftRepository.create_shift(
        conn, shift_id='shift-cw4', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    # Seed cash balance so CASH_WITHDRAWAL guard passes
    import json as _json
    _pj = _json.dumps({'service_sum': 999999})
    conn.execute("INSERT INTO ingress_inbox (request_id, idempotency_key, protocol, operation_type, fiscal_number, backend_profile_id, transport_profile_id, channel_owner, payload_json, payload_sha256, status, response_deadline_at) VALUES ('req-cw4-seed', 'idem-cw4-seed', 'CHECKBOX_REST', 'SERVICE_IN', 'FN-DEV-0001', 'backend_checkbox_default', 'transport_dps_grpc_default', 'test', ?, 'sha', 'DONE', '2026-04-14T23:00:00Z')", (_pj,))
    conn.execute("INSERT INTO fiscal_documents (document_id, request_id, fiscal_number, doc_type, state, fs_mode, lnd, backend_profile_id, transport_profile_id, submission_status, payload_json, payload_sha256, business_ts, shift_id) VALUES ('doc-cw4-seed', 'req-cw4-seed', 'FN-DEV-0001', 'SERVICE_IN', 'ACK', 'ONLINE', 500, 'backend_checkbox_default', 'transport_dps_grpc_default', 'DPS_ACK', ?, 'sha', '2026-04-14T08:00:00Z', 'shift-cw4')", (_pj,))
    conn.commit()

    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-cw4', 'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_dps_grpc_default',
            'channel_owner': 'test', 'business_ts': '2026-04-14T14:00:00Z',
        },
        'operation': 'CASH_WITHDRAWAL',
        'request': {
            'payments': [{'type': 'CASH', 'value': 20000,
                          'bank_name': 'Mono', 'card_mask': 'VISA ****5678',
                          'rrn': '123456789', 'auth_code': '999'}],
        },
    })

    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T14:01:00Z',
    )
    conn.commit()

    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-cw4',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'

    xml = signed_xmls[0]
    assert '<C T="8">' in xml, f'Must be T=8 in signed XML. Got: {xml}'
    assert 'NM="Гривня"' in xml
    assert 'SM="20000"' in xml
