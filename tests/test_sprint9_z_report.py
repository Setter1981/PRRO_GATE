"""Sprint 9 step 5: full Z-report with TXS, M, IO, NC.

Tests cover:
  ZR1: Z-report without z_report_data → minimal (backward compat)
  ZR2: Z-report with tax_sums → TXS blocks with calculated TXI
  ZR3: Z-report with payment_sums → M blocks
  ZR4: Z-report with service_sums → IO blocks
  ZR5: Z-report with check_count → NC block
  ZR6: full pipeline — SELL+RETURN+SERVICE_IN+Z_REPORT aggregates correctly
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml
from prro_gateway.repositories.tax_groups import TaxGroupDef


def _group(tax_id, name='A', tax_rate=20.0, additional_rate=0.0, tax_algorithm=0):
    return TaxGroupDef(
        fiscal_number='FN', tax_id=tax_id, name=name,
        tax_rate=tax_rate, additional_rate=additional_rate,
        tax_type=0, tax_algorithm=tax_algorithm,
        requires_uktzed=False, requires_excise_mark=False, is_active=True,
    )


# ---------------------------------------------------------------------------
# ZR1 — no z_report_data → minimal Z
# ---------------------------------------------------------------------------

def test_zr1_minimal_z_without_data() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001', local_number=10,
        business_ts=datetime(2026, 4, 14, 22, 0, 0, tzinfo=UTC),
        tax_number='TN', z_number=5,
    )
    assert '<Z NO="5"' in xml
    assert '<TXS' not in xml


# ---------------------------------------------------------------------------
# ZR2 — tax_sums → TXS blocks
# ---------------------------------------------------------------------------

def test_zr2_txs_blocks() -> None:
    groups = {
        '1': _group('1', name='А', tax_rate=20.0),
        '4': _group('4', name='ГА', tax_rate=20.0, additional_rate=5.0, tax_algorithm=2),
    }
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001', local_number=10,
        business_ts=datetime(2026, 4, 14, 22, 0, 0, tzinfo=UTC),
        tax_number='TN', z_number=5,
        tax_groups=groups,
        payload={'z_report_data': {
            'tax_sums': {
                '1': {'smi': 120000, 'smo': 5000},
                '4': {'smi': 29560, 'smo': 0},
            },
        }},
    )
    assert '<TXS' in xml
    import re
    txs_blocks = re.findall(r'<TXS [^>]+>', xml)
    assert len(txs_blocks) == 2, f'Expected 2 TXS blocks, got {len(txs_blocks)}'
    assert 'TX="1"' in xml
    assert 'TX="4"' in xml
    assert 'SMI="120000"' in xml
    assert 'SMI="29560"' in xml


# ---------------------------------------------------------------------------
# ZR3 — payment_sums → M blocks
# ---------------------------------------------------------------------------

def test_zr3_m_blocks() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001', local_number=10,
        business_ts=datetime(2026, 4, 14, 22, 0, 0, tzinfo=UTC),
        tax_number='TN', z_number=5,
        payload={'z_report_data': {
            'payment_sums': {
                'CASH': {'smi': 200000, 'smo': 15000},
                'CASHLESS': {'smi': 350000, 'smo': 0},
            },
        }},
    )
    assert '<M NM="CASH"' in xml
    assert '<M NM="CASHLESS"' in xml
    assert 'SMI="200000"' in xml
    assert 'SMO="15000"' in xml


# ---------------------------------------------------------------------------
# ZR4 — service_sums → IO blocks
# ---------------------------------------------------------------------------

def test_zr4_io_blocks() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001', local_number=10,
        business_ts=datetime(2026, 4, 14, 22, 0, 0, tzinfo=UTC),
        tax_number='TN', z_number=5,
        payload={'z_report_data': {
            'service_sums': {'ГОТІВКА': {'smi': 50000, 'smo': 180000}},
        }},
    )
    assert '<IO' in xml
    assert 'SMI="50000"' in xml
    assert 'SMO="180000"' in xml


# ---------------------------------------------------------------------------
# ZR5 — check_count → NC block
# ---------------------------------------------------------------------------

def test_zr5_nc_block() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001', local_number=10,
        business_ts=datetime(2026, 4, 14, 22, 0, 0, tzinfo=UTC),
        tax_number='TN', z_number=5,
        payload={'z_report_data': {
            'check_count': {'ni': 42, 'no': 3},
        }},
    )
    assert '<NC NI="42" NO="3"></NC>' in xml


# ---------------------------------------------------------------------------
# ZR6 — full pipeline: SELL+RETURN+SERVICE_IN → Z_REPORT aggregates
# ---------------------------------------------------------------------------

def test_zr6_full_pipeline_z_aggregation(conn) -> None:
    """Create SELL, RETURN, SERVICE_IN via write_path, then Z_REPORT.
    Z XML must contain aggregated TXS, M, IO, NC."""
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    # Seed tax group
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
        (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
         requires_uktzed, requires_excise_mark)
        VALUES ('FN-DEV-0001', '1', 'А', 20.00, 0, 0, 0, 0, 0)
    """)
    conn.commit()

    shift_id = 'shift-zr6'
    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()

    def _mk_cmd(rid, op, payload):
        return CanonicalFiscalCommand(
            request_id=rid, idempotency_key=f'idem-{rid}',
            protocol=Protocol.CHECKBOX_REST, operation_type=op,
            fiscal_number='FN-DEV-0001', route_key='main',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            channel_owner='test', external_request_id=f'ext-{rid}',
            business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
            payload=payload, payload_sha256=f'sha-{rid}',
            trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{rid}'),
            correlation_id=f'c-{rid}',
        )

    def _enq(c, rid, op, payload):
        cmd = _mk_cmd(rid, op, payload)
        c.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            c, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
            protocol=cmd.protocol, operation_type=cmd.operation_type,
            fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
            backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
            channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
            protocol_session_id=None, payload_json=cmd.model_dump_json(),
            payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T23:00:00Z',
        )
        c.commit()

    signed_xmls = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_xmls.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id=f'tr-{kw["document_id"]}',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')

    # 1. SELL: 2 items, group 1, total 30000 (300 грн), cash
    _enq(conn, 'req-zr6-sell', OperationType.SELL, {
        'receipt': {
            'type': 'SELL',
            'goods': [
                {'name': 'Bread', 'price': 5000, 'quantity': 2000, 'sum': 10000, 'tax_id': '1'},
                {'name': 'Milk', 'price': 20000, 'quantity': 1000, 'sum': 20000, 'tax_id': '1'},
            ],
            'payments': [{'amount': 30000, 'type': 'CASH'}],
            'totals': {'total_sum': 30000},
        },
    })
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # 2. RETURN: 1 item, group 1, total 5000, cash
    _enq(conn, 'req-zr6-ret', OperationType.RETURN, {
        'receipt': {
            'type': 'RETURN',
            'goods': [{'name': 'Bread', 'price': 5000, 'quantity': 1000, 'sum': 5000, 'tax_id': '1'}],
            'payments': [{'amount': 5000, 'type': 'CASH'}],
            'totals': {'total_sum': 5000},
            'related_receipt_id': 'orig-001',
        },
    })
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # 3. SERVICE_IN: 50000 (500 грн)
    _enq(conn, 'req-zr6-svc', OperationType.SERVICE_IN, {
        'service_sum': 50000,
        'receipt': {'type': 'SERVICE_IN', 'goods': [], 'payments': [], 'totals': {}},
    })
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # 4. Z_REPORT
    signed_xmls.clear()
    _enq(conn, 'req-zr6-z', OperationType.Z_REPORT, {
        'receipt': {'type': 'Z_REPORT', 'goods': [], 'payments': [], 'totals': {}},
    })
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Z_REPORT should ACK. Got: {result.canonical_error}'

    # Verify Z XML content
    z_xml = signed_xmls[0]
    assert '<Z NO=' in z_xml, f'Must have Z element. Got: {z_xml}'

    # TXS: group 1, SMI=30000 (sells), SMO=5000 (returns)
    assert '<TXS' in z_xml, f'Must have TXS. Got: {z_xml}'
    assert 'SMI="30000"' in z_xml, f'TXS SMI must be 30000. Got: {z_xml}'
    assert 'SMO="5000"' in z_xml, f'TXS SMO must be 5000. Got: {z_xml}'

    # M: CASH SMI=30000, SMO=5000
    assert '<M NM="CASH"' in z_xml, f'Must have M block. Got: {z_xml}'

    # IO: ГОТІВКА SMI=50000 (service_in)
    assert '<IO' in z_xml, f'Must have IO block. Got: {z_xml}'
    assert 'SMI="50000"' in z_xml

    # NC: 1 sell, 1 return
    assert '<NC NI="1" NO="1"></NC>' in z_xml, f'Must have NC NI=1 NO=1. Got: {z_xml}'
