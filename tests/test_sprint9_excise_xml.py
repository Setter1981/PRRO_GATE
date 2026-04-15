"""Sprint 9 step 6: CZD and CA serialization in DPS XML.

Tests cover:
  EX_XML1: CZD attribute on P when uktzed provided
  EX_XML2: CA sub-elements in P when excise_barcodes provided
  EX_XML3: 3 bottles = 3 CA elements
  EX_XML4: no CZD/CA when not provided (backward compat)
  EX_XML5: full pipeline — uktzed+marks from ingress reach signed XML
  EX_XML6: CD (barcode) separate from CZD (uktzed)
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


# ---------------------------------------------------------------------------
# EX_XML1 — CZD attribute
# ---------------------------------------------------------------------------

def test_ex_xml1_czd_attribute() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
                                        'uktzed': '2208909900', 'tax_id': '4'}],
                             'payments': [{'amount': 29560, 'type': 'CASH'}],
                             'totals': {'total_sum': 29560}}},
        tax_number='TN',
    )
    assert 'CZD="2208909900"' in xml, f'CZD must be present. Got: {xml}'


# ---------------------------------------------------------------------------
# EX_XML2 — CA sub-elements
# ---------------------------------------------------------------------------

def test_ex_xml2_ca_elements() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
                                        'uktzed': '2208909900', 'tax_id': '4',
                                        'excise_barcodes': ['FRTG456985']}],
                             'payments': [{'amount': 29560, 'type': 'CASH'}],
                             'totals': {'total_sum': 29560}}},
        tax_number='TN',
    )
    assert '<CA CA="FRTG456985"></CA>' in xml, f'CA must be present. Got: {xml}'
    assert '</P>' in xml, f'P must be closed tag (not self-closing) when CA present. Got: {xml}'


# ---------------------------------------------------------------------------
# EX_XML3 — 3 bottles = 3 CA
# ---------------------------------------------------------------------------

def test_ex_xml3_three_bottles_three_ca() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'Горілка', 'price': 29560, 'quantity': 3000, 'sum': 88680,
                                        'uktzed': '2208909900', 'tax_id': '4',
                                        'excise_barcodes': ['FRTG456985', 'FRTG456986', 'FRTG456987']}],
                             'payments': [{'amount': 88680, 'type': 'CASH'}],
                             'totals': {'total_sum': 88680}}},
        tax_number='TN',
    )
    import re
    ca_elements = re.findall(r'<CA CA="[^"]+">', xml)
    assert len(ca_elements) == 3, f'Expected 3 CA elements, got {len(ca_elements)}: {xml}'
    assert 'FRTG456985' in xml
    assert 'FRTG456986' in xml
    assert 'FRTG456987' in xml


# ---------------------------------------------------------------------------
# EX_XML4 — no CZD/CA when not provided
# ---------------------------------------------------------------------------

def test_ex_xml4_no_czd_ca_when_absent() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'Хліб', 'price': 3500, 'quantity': 1000, 'sum': 3500}],
                             'payments': [{'amount': 3500, 'type': 'CASH'}],
                             'totals': {'total_sum': 3500}}},
        tax_number='TN',
    )
    assert 'CZD=' not in xml
    assert '<CA ' not in xml
    assert '</P>' in xml  # canonical closing


# ---------------------------------------------------------------------------
# EX_XML5 — full pipeline: uktzed+marks reach signed XML
# ---------------------------------------------------------------------------

def test_ex_xml5_full_pipeline(conn) -> None:
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    # Seed excise group with excise_allowed
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
        (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
         requires_uktzed, requires_excise_mark)
        VALUES ('FN-DEV-0001', '4', 'ГА', 20.00, 5.00, 0, 2, 1, 1)
    """)
    conn.execute("UPDATE node_state SET excise_allowed = 1 WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()

    ShiftRepository.create_shift(
        conn, shift_id='shift-exml5', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()

    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter
    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-exml5', 'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_dps_grpc_default',
            'channel_owner': 'test', 'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [{'good': {'name': 'Горілка Finlandia', 'tax_id': '4', 'uktzed': '2208909900'},
                        'price': 29560, 'quantity': 2000,
                        'excise_barcodes': ['FRTG456985', 'FRTG456986']}],
            'payments': [{'type': 'CASH', 'value': 59120}],
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
            return SendResult(state='ACK', transport_request_id='tr',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'

    xml = signed_xmls[0]
    assert 'CZD="2208909900"' in xml, f'CZD must be in signed XML. Got: {xml}'
    assert '<CA CA="FRTG456985"></CA>' in xml, f'CA mark 1 must be in XML. Got: {xml}'
    assert '<CA CA="FRTG456986"></CA>' in xml, f'CA mark 2 must be in XML. Got: {xml}'
    assert 'TX="4"' in xml


# ---------------------------------------------------------------------------
# EX_XML6 — CD (barcode) separate from CZD (uktzed)
# ---------------------------------------------------------------------------

def test_ex_xml6_cd_and_czd_separate() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
                                        'barcode': '4820000000001', 'uktzed': '2208909900'}],
                             'payments': [{'amount': 29560, 'type': 'CASH'}],
                             'totals': {'total_sum': 29560}}},
        tax_number='TN',
    )
    assert 'CD="4820000000001"' in xml, f'CD must be barcode. Got: {xml}'
    assert 'CZD="2208909900"' in xml, f'CZD must be uktzed. Got: {xml}'
