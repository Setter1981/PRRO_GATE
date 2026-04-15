"""Sprint 10 step 8: EPZ attributes on non-cash <M>.

Tests:
  EPZ1: non-cash M has RRN/PSNM/PA/PB/PD/PE in signed XML
  EPZ2: empty fields not serialized
  EPZ3: payment type resolution (T and NM from definitions)
  EPZ4: CASH unchanged (no EPZ attrs)
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


def _setup(conn):
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = 'shift-epz'").fetchone()
    if not existing:
        ShiftRepository.create_shift(
            conn, shift_id='shift-epz', fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-14T08:00:00Z',
        )
        conn.commit()


def _seed_payment_type(conn, type_code, type_group, name):
    conn.execute(
        "INSERT OR IGNORE INTO payment_type_definitions (fiscal_number, type_code, type_group, name) VALUES ('FN-DEV-0001', ?, ?, ?)",
        (type_code, type_group, name),
    )
    conn.commit()


def _enqueue_sell(conn, rid, payments, total_sum=10000):
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
                'goods': [{'name': 'Item', 'price': total_sum, 'quantity': 1000, 'sum': total_sum}],
                'payments': payments,
                'totals': {'total_sum': total_sum},
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


class _SpyCrypto:
    def __init__(self):
        self.signed = []
    def sign(self, *, payload_json, **kw):
        self.signed.append(payload_json)
        return f'SIGNED::{payload_json[:40]}'


class _OkTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(state='ACK', transport_request_id='tr',
                          submission_status='DPS_ACK', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


# ---------------------------------------------------------------------------
# EPZ1 — non-cash M has EPZ attrs
# ---------------------------------------------------------------------------

def test_epz1_non_cash_has_epz_attrs(conn) -> None:
    _setup(conn)
    _seed_payment_type(conn, 'CARD', 2, 'Картка')
    _enqueue_sell(conn, 'req-epz1', [{
        'type': 'CARD', 'amount': 10000,
        'rrn': '044702496008', 'payment_system': 'VISA',
        'bank_name': 'ПриватБанк', 'card_mask': 'VISA ****1234',
        'auth_code': '390546', 'terminal': 'TERM001',
    }])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'T="2"' in xml
    assert 'NM="Картка"' in xml
    assert 'RRN="044702496008"' in xml
    assert 'PSNM="VISA"' in xml
    assert 'PA="ПриватБанк"' in xml
    assert 'PD="VISA ****1234"' in xml
    assert 'PE="390546"' in xml
    assert 'PB="TERM001"' in xml


# ---------------------------------------------------------------------------
# EPZ2 — empty fields not serialized
# ---------------------------------------------------------------------------

def test_epz2_empty_fields_not_serialized(conn) -> None:
    _setup(conn)
    _seed_payment_type(conn, 'CARD2', 2, 'Картка')
    _enqueue_sell(conn, 'req-epz2', [{
        'type': 'CARD2', 'amount': 10000,
        'rrn': '123456', 'card_mask': 'MC ****9999',
    }])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'RRN="123456"' in xml
    assert 'PD="MC ****9999"' in xml
    assert 'PA=' not in xml, f'Empty PA must not appear. Got: {xml}'
    assert 'PB=' not in xml, f'Empty PB must not appear. Got: {xml}'
    assert 'PE=' not in xml, f'Empty PE must not appear. Got: {xml}'
    assert 'PSNM=' not in xml, f'Empty PSNM must not appear. Got: {xml}'


# ---------------------------------------------------------------------------
# EPZ3 — payment type resolution
# ---------------------------------------------------------------------------

def test_epz3_payment_type_resolution(conn) -> None:
    _setup(conn)
    _seed_payment_type(conn, 'BONUS', 1, 'Бонусний рахунок')
    _enqueue_sell(conn, 'req-epz3', [{'type': 'BONUS', 'amount': 10000}])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'T="1"' in xml, f'BONUS type_group=1 → T="1". Got: {xml}'
    assert 'NM="Бонусний рахунок"' in xml, f'NM from definitions. Got: {xml}'


# ---------------------------------------------------------------------------
# EPZ4 — CASH unchanged
# ---------------------------------------------------------------------------

def test_epz4_cash_no_epz(conn) -> None:
    _setup(conn)
    _enqueue_sell(conn, 'req-epz4', [{
        'type': 'CASH', 'amount': 10000,
        'rrn': 'should-not-appear', 'card_mask': 'nope',
    }])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'T="0"' in xml
    assert 'NM="Готівка"' in xml
    assert 'RRN=' not in xml, f'CASH must NOT have RRN. Got: {xml}'
    assert 'PD=' not in xml, f'CASH must NOT have PD. Got: {xml}'


# ---------------------------------------------------------------------------
# EPZ5 — PC (label) and PF (commission) serialized
# ---------------------------------------------------------------------------

def test_epz5_pc_and_pf_serialized(conn) -> None:
    _setup(conn)
    _seed_payment_type(conn, 'MONO', 2, 'Монобанк')
    _enqueue_sell(conn, 'req-epz5', [{
        'type': 'MONO', 'amount': 10000,
        'label': 'ОПЛАТА', 'commission': 150,
        'rrn': '999', 'card_mask': 'MONO ****0001',
    }])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'PC="ОПЛАТА"' in xml, f'PC (label) must be present. Got: {xml}'
    assert 'PF="150"' in xml, f'PF (commission) must be present. Got: {xml}'


# ---------------------------------------------------------------------------
# EPZ6 — negative commission not serialized
# ---------------------------------------------------------------------------

def test_epz6_negative_commission_not_serialized(conn) -> None:
    _setup(conn)
    _seed_payment_type(conn, 'NEG', 2, 'Test')
    _enqueue_sell(conn, 'req-epz6', [{
        'type': 'NEG', 'amount': 10000,
        'commission': -50, 'rrn': '111',
    }])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'PF=' not in xml, f'Negative commission must NOT appear. Got: {xml}'
