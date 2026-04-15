"""Sprint 9 step 2: TX (tax group) on P element tests.

Tests cover:
  TG1: P element includes TX when tax_id provided (serializer)
  TG2: P element omits TX when tax_id not provided (backward compat)
  TG3: multiple goods with different tax groups
  TG3b: TX="0" (exempt) and TX="-1" (not taxable) must serialize
  TG4: adapter passes tax_id from request
  TG5: full write-path: TX from ingress reaches signed DPS XML
  TG6: TX="0" through full pipeline is not lost
  TG7: goods without tax_id still ACK through write-path (backward compat)
  TG8: TX1 (excise/second tax) serialized on P element
  TG9: TX + TX1 together (alcohol: ПДВ + акциз) through full pipeline
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


def test_tg1_p_includes_tx_when_provided() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Bread', 'price': 3500, 'quantity': 1000, 'sum': 3500, 'tax_id': '1'}],
                'payments': [{'amount': 3500, 'type': 'CASH'}],
                'totals': {'total_sum': 3500},
            },
        },
        tax_number='TN-001',
    )
    assert 'TX="1"' in xml, f'P must include TX="1" when tax_id provided. Got: {xml}'


def test_tg2_p_omits_tx_when_not_provided() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Water', 'price': 1500, 'quantity': 1000, 'sum': 1500}],
                'payments': [{'amount': 1500, 'type': 'CASH'}],
                'totals': {'total_sum': 1500},
            },
        },
        tax_number='TN-001',
    )
    assert ' TX="' not in xml, f'P must NOT include TX when tax_id absent. Got: {xml}'


def test_tg3_multiple_tax_groups() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=2,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [
                    {'name': 'Bread', 'price': 3500, 'quantity': 1000, 'sum': 3500, 'tax_id': '1'},
                    {'name': 'Wine', 'price': 25000, 'quantity': 1000, 'sum': 25000, 'tax_id': '5'},
                ],
                'payments': [{'amount': 28500, 'type': 'CASH'}],
                'totals': {'total_sum': 28500},
            },
        },
        tax_number='TN-001',
    )
    assert 'TX="1"' in xml
    assert 'TX="5"' in xml


def test_tg3b_tax_zero_and_minus_one() -> None:
    """TX="0" (exempt) and TX="-1" (not taxable) must serialize, not be skipped as falsy."""
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=3,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [
                    {'name': 'Exempt item', 'price': 1000, 'quantity': 1000, 'sum': 1000, 'tax_id': '0'},
                    {'name': 'Not taxable', 'price': 2000, 'quantity': 1000, 'sum': 2000, 'tax_id': '-1'},
                ],
                'payments': [{'amount': 3000, 'type': 'CASH'}],
                'totals': {'total_sum': 3000},
            },
        },
        tax_number='TN-001',
    )
    assert 'TX="0"' in xml, f'TX="0" (exempt) must be present. Got: {xml}'
    assert 'TX="-1"' in xml, f'TX="-1" (not taxable) must be present. Got: {xml}'


def test_tg4_adapter_passes_tax_id() -> None:
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-tg4',
            'fiscal_number': 'FN-001',
            'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [
                {
                    'good': {'name': 'Beer', 'tax_id': '5'},
                    'price': 5000,
                    'quantity': 1000,
                    'sum': 5000,
                },
            ],
            'payments': [{'type': 'CASH', 'value': 5000}],
        },
    })
    goods = cmd.payload.get('receipt', {}).get('goods', [])
    assert len(goods) == 1
    assert goods[0]['tax_id'] == '5', f'Adapter must pass tax_id. Got: {goods[0]}'


# ---------------------------------------------------------------------------
# TG5 — full write-path: TX from ingress reaches signed DPS XML
# ---------------------------------------------------------------------------

def test_tg5_full_pipeline_tx_in_signed_xml(conn) -> None:
    """Tax group from ingress request must survive the entire pipeline:
    adapter → canonical → inbox → write_path → serializer → signed XML.
    """
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    # 1. Adapter: build canonical command with tax_id
    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-tg5',
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_dps_grpc_default',
            'channel_owner': 'test',
            'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [
                {'good': {'name': 'Wine', 'tax_id': '5'}, 'price': 25000, 'quantity': 1000, 'sum': 25000},
                {'good': {'name': 'Bread', 'tax_id': '1'}, 'price': 3500, 'quantity': 2000, 'sum': 7000},
            ],
            'payments': [{'type': 'CASH', 'value': 32000}],
        },
    })

    # Verify adapter preserved tax_id in canonical
    goods = cmd.payload['receipt']['goods']
    assert goods[0]['tax_id'] == '5'
    assert goods[1]['tax_id'] == '1'

    # 2. Store in inbox
    ShiftRepository.create_shift(
        conn, shift_id='shift-tg5', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()
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

    # 3. Process through write-path with spy crypto
    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-tg5',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN-TG5')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'

    # 4. Verify TX in the signed XML
    assert len(signed_xmls) == 1
    xml = signed_xmls[0]
    assert 'TX="5"' in xml, f'Wine TX="5" must be in signed XML. Got: {xml}'
    assert 'TX="1"' in xml, f'Bread TX="1" must be in signed XML. Got: {xml}'
    assert 'NM="Wine"' in xml
    assert 'NM="Bread"' in xml


# ---------------------------------------------------------------------------
# TG6 — TX="0" through full pipeline is not lost
# ---------------------------------------------------------------------------

def test_tg6_tx_zero_survives_pipeline(conn) -> None:
    """TX="0" (exempt from tax) must not be dropped as falsy anywhere in pipeline."""
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-tg6',
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_dps_grpc_default',
            'channel_owner': 'test',
            'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [{'good': {'name': 'Exempt item', 'tax_id': '0'}, 'price': 1000, 'quantity': 1000, 'sum': 1000}],
            'payments': [{'type': 'CASH', 'value': 1000}],
        },
    })
    assert cmd.payload['receipt']['goods'][0]['tax_id'] == '0'

    ShiftRepository.create_shift(
        conn, shift_id='shift-tg6', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()
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

    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-tg6',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN-TG6')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert len(signed_xmls) == 1
    assert 'TX="0"' in signed_xmls[0], f'TX="0" must survive full pipeline. Got: {signed_xmls[0]}'


# ---------------------------------------------------------------------------
# TG7 — goods without tax_id still work (backward compat through write-path)
# ---------------------------------------------------------------------------

def test_tg7_no_tax_id_backward_compat(conn) -> None:
    """Goods without tax_id must still produce valid XML and ACK through write-path."""
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-tg7',
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_dps_grpc_default',
            'channel_owner': 'test',
            'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [{'good': {'name': 'No tax item'}, 'price': 500, 'quantity': 1000, 'sum': 500}],
            'payments': [{'type': 'CASH', 'value': 500}],
        },
    })
    assert cmd.payload['receipt']['goods'][0].get('tax_id') is None

    ShiftRepository.create_shift(
        conn, shift_id='shift-tg7', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()
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

    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-tg7',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN-TG7')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert len(signed_xmls) == 1
    assert ' TX="' not in signed_xmls[0], f'No TX attribute when tax_id absent. Got: {signed_xmls[0]}'


# ---------------------------------------------------------------------------
# TG8 — TX1 (excise/second tax) serialized on P element
# ---------------------------------------------------------------------------

def test_tg8_tx1_excise_on_p() -> None:
    """TX1 = second tax group (excise). Must serialize as TX1="..." on P."""
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Горілка Finlandia', 'price': 29560, 'quantity': 1000, 'sum': 29560,
                           'tax_id': '1', 'tax_id_2': '2'}],
                'payments': [{'amount': 29560, 'type': 'CASH'}],
                'totals': {'total_sum': 29560},
            },
        },
        tax_number='TN-001',
    )
    assert 'TX="1"' in xml, f'ПДВ TX="1" must be present. Got: {xml}'
    assert 'TX1="2"' in xml, f'Акциз TX1="2" must be present. Got: {xml}'


# ---------------------------------------------------------------------------
# TG9 — TX + TX1 together through full write-path pipeline
# ---------------------------------------------------------------------------

def test_tg9_tx_and_tx1_full_pipeline(conn) -> None:
    """Alcohol sale: TX="1" (ПДВ) + TX1="2" (акциз) must both survive
    adapter → inbox → write_path → serializer → signed XML."""
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-tg9',
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_dps_grpc_default',
            'channel_owner': 'test',
            'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [
                {'good': {'name': 'Горілка Finlandia', 'tax_id': '1', 'tax_id_2': '2'},
                 'price': 29560, 'quantity': 1000, 'sum': 29560},
                {'good': {'name': 'Ковбаса Алан', 'tax_id': '1'},
                 'price': 26995, 'quantity': 1000, 'sum': 26995},
            ],
            'payments': [{'type': 'CASH', 'value': 56555}],
        },
    })

    # Verify adapter preserved both tax fields
    goods = cmd.payload['receipt']['goods']
    assert goods[0]['tax_id'] == '1'
    assert goods[0]['tax_id_2'] == '2'
    assert goods[1]['tax_id'] == '1'
    assert goods[1].get('tax_id_2') is None

    ShiftRepository.create_shift(
        conn, shift_id='shift-tg9', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()
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

    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-tg9',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN-TG9')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert len(signed_xmls) == 1
    xml = signed_xmls[0]

    # Горілка: TX="1" TX1="2"
    assert 'TX="1" TX1="2"' in xml or 'TX="1"' in xml and 'TX1="2"' in xml, \
        f'Горілка must have TX="1" and TX1="2". Got: {xml}'
    # Ковбаса: TX="1" only, no TX1
    # Find the second P element and verify no TX1
    import re
    p_elements = re.findall(r'<P [^>]+>', xml)
    assert len(p_elements) == 2, f'Expected 2 P elements, got {len(p_elements)}'
    assert 'TX1=' in p_elements[0], f'First P (горілка) must have TX1. Got: {p_elements[0]}'
    assert 'TX1=' not in p_elements[1], f'Second P (ковбаса) must NOT have TX1. Got: {p_elements[1]}'


# ---------------------------------------------------------------------------
# TG10 — non-VAT payer with excise: TX="-1" + TX1="2"
# ---------------------------------------------------------------------------

def test_tg10_non_vat_payer_with_excise(conn) -> None:
    """Non-VAT payer selling alcohol: TX="-1" (not taxable) + TX1="2" (excise 5%).
    Both must survive full pipeline."""
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-tg10',
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_dps_grpc_default',
            'channel_owner': 'test',
            'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [
                {'good': {'name': 'Горілка', 'tax_id': '-1', 'tax_id_2': '2'},
                 'price': 29560, 'quantity': 1000, 'sum': 29560},
            ],
            'payments': [{'type': 'CASH', 'value': 29560}],
        },
    })

    assert cmd.payload['receipt']['goods'][0]['tax_id'] == '-1'
    assert cmd.payload['receipt']['goods'][0]['tax_id_2'] == '2'

    ShiftRepository.create_shift(
        conn, shift_id='shift-tg10', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()
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

    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-tg10',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN-TG10')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    xml = signed_xmls[0]
    assert 'TX="-1"' in xml, f'TX="-1" must survive pipeline. Got: {xml}'
    assert 'TX1="2"' in xml, f'TX1="2" (excise) must survive pipeline. Got: {xml}'
