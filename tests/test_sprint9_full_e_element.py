"""Sprint 9 step 4: full <E> element with tax calculation.

Tests cover:
  FE1: <E> without tax_groups — minimal (backward compat)
  FE2: <E> with one tax group — full attributes + <TX> block
  FE3: <E> with two tax groups — two <TX> blocks, correct per-group TXSM
  FE4: TXAL=0 calculation (ПДВ included in price)
  FE5: TXAL=2 calculation (excise on price+VAT)
  FE6: full pipeline — tax_groups from DB reach <E> in signed XML
  FE7: <E> includes FN, NO, TS attributes
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml, _calc_tax
from prro_gateway.repositories.tax_groups import TaxGroupDef


def _group(tax_id, name='A', tax_rate=20.0, additional_rate=0.0, tax_algorithm=0):
    return TaxGroupDef(
        fiscal_number='FN', tax_id=tax_id, name=name,
        tax_rate=tax_rate, additional_rate=additional_rate,
        tax_type=0, tax_algorithm=tax_algorithm,
        requires_uktzed=False, requires_excise_mark=False, is_active=True,
    )


# ---------------------------------------------------------------------------
# FE1 — no tax_groups → minimal <E>
# ---------------------------------------------------------------------------

def test_fe1_minimal_e_without_groups() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                             'payments': [{'amount': 1000, 'type': 'CASH'}],
                             'totals': {'total_sum': 1000}}},
        tax_number='TN',
    )
    assert '<E N="3" SM="1000"></E>' in xml
    assert '<TX ' not in xml


# ---------------------------------------------------------------------------
# FE2 — one tax group → full <E> with one <TX>
# ---------------------------------------------------------------------------

def test_fe2_full_e_with_one_group() -> None:
    groups = {'1': _group('1', name='А', tax_rate=20.0)}
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=5,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'Bread', 'price': 12000, 'quantity': 1000, 'sum': 12000, 'tax_id': '1'}],
                             'payments': [{'amount': 12000, 'type': 'CASH'}],
                             'totals': {'total_sum': 12000}}},
        tax_number='TN', tax_groups=groups,
    )
    # Must have full <E> with FN, NO, SM, TS
    assert 'FN="FN-001"' in xml
    assert 'SM="12000"' in xml
    assert '<TX ' in xml
    assert 'TX="1"' in xml
    assert 'TXPR="20.00"' in xml
    # TXSM = 12000 * 20 / 120 = 2000
    assert 'TXSM="2000"' in xml


# ---------------------------------------------------------------------------
# FE3 — two tax groups → two <TX> blocks
# ---------------------------------------------------------------------------

def test_fe3_two_groups_two_tx_blocks() -> None:
    groups = {
        '1': _group('1', name='А', tax_rate=20.0),
        '2': _group('2', name='Б', tax_rate=7.0),
    }
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=6,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [
                                 {'name': 'Bread', 'price': 12000, 'quantity': 1000, 'sum': 12000, 'tax_id': '1'},
                                 {'name': 'Medicine', 'price': 10700, 'quantity': 1000, 'sum': 10700, 'tax_id': '2'},
                             ],
                             'payments': [{'amount': 22700, 'type': 'CASH'}],
                             'totals': {'total_sum': 22700}}},
        tax_number='TN', tax_groups=groups,
    )
    import re
    tx_blocks = re.findall(r'<TX [^>]+>', xml)
    assert len(tx_blocks) == 2, f'Expected 2 <TX> blocks, got {len(tx_blocks)}: {xml}'
    assert 'TX="1"' in xml
    assert 'TX="2"' in xml
    # Group 1: TXSM = 12000 * 20 / 120 = 2000
    assert 'TXSM="2000"' in xml
    # Group 2: TXSM = 10700 * 7 / 107 = 700
    assert 'TXSM="700"' in xml


# ---------------------------------------------------------------------------
# FE4 — TXAL=0 (standard VAT included in price)
# ---------------------------------------------------------------------------

def test_fe4_txal0_vat_included() -> None:
    txsm, dtsm = _calc_tax(12000, tax_rate=20.0, additional_rate=0.0, tax_algorithm=0)
    assert txsm == 2000  # 12000 * 20 / 120
    assert dtsm == 0


# ---------------------------------------------------------------------------
# FE5 — TXAL=2 (excise on price including VAT)
# ---------------------------------------------------------------------------

def test_fe5_txal2_excise_on_price_with_vat() -> None:
    # Example from ФСКО: горілка SM=29560, TXPR=20%, DTPR=5%, TXAL=2
    txsm, dtsm = _calc_tax(29560, tax_rate=20.0, additional_rate=5.0, tax_algorithm=2)
    # DTSM = 29560 * 5 / 105 = 1407.6... → 1408
    assert dtsm == 1408, f'DTSM expected 1408, got {dtsm}'
    # TXSM = (29560 - 1408) * 20 / 120 = 28152 * 20 / 120 = 4692
    assert txsm == 4692, f'TXSM expected 4692, got {txsm}'


# ---------------------------------------------------------------------------
# FE6 — full pipeline: tax_groups from DB reach <E> in signed XML
# ---------------------------------------------------------------------------

def test_fe6_full_pipeline_e_with_tax(conn) -> None:
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    # Seed tax groups
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
        (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
         requires_uktzed, requires_excise_mark)
        VALUES ('FN-DEV-0001', '1', 'А', 20.00, 0, 0, 0, 0, 0)
    """)
    conn.commit()

    ShiftRepository.create_shift(
        conn, shift_id='shift-fe6', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-fe6', idempotency_key='idem-fe6',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-fe6',
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Товар', 'price': 24000, 'quantity': 1000, 'sum': 24000, 'tax_id': '1'}],
                'payments': [{'amount': 24000, 'type': 'CASH'}],
                'totals': {'total_sum': 24000},
            },
        },
        payload_sha256='sha-fe6',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-fe6'),
        correlation_id='c-fe6',
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

    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-fe6',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    xml = signed_xmls[0]

    # Full <E> with tax data from DB
    assert '<TX ' in xml, f'<TX> block must be in signed XML. Got: {xml}'
    assert 'TXPR="20.00"' in xml
    # TXSM = 24000 * 20 / 120 = 4000
    assert 'TXSM="4000"' in xml
    assert 'FN="FN-DEV-0001"' in xml  # <E> has FN attribute


# ---------------------------------------------------------------------------
# FE7 — <E> includes NO and TS attributes
# ---------------------------------------------------------------------------

def test_fe7_e_has_no_and_ts() -> None:
    groups = {'1': _group('1', name='А', tax_rate=20.0)}
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=42,
        business_ts=datetime(2026, 4, 14, 15, 30, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL',
                             'goods': [{'name': 'X', 'price': 6000, 'quantity': 1000, 'sum': 6000, 'tax_id': '1'}],
                             'payments': [{'amount': 6000, 'type': 'CASH'}],
                             'totals': {'total_sum': 6000}}},
        tax_number='TN', tax_groups=groups,
    )
    import re
    e_match = re.search(r'<E [^>]+>', xml)
    assert e_match, f'<E> element not found in: {xml}'
    e_tag = e_match.group()
    assert 'NO="' in e_tag, f'<E> must have NO attribute. Got: {e_tag}'
    assert 'TS="' in e_tag, f'<E> must have TS attribute. Got: {e_tag}'
    assert 'FN="FN-001"' in e_tag
