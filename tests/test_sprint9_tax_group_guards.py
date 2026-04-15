"""Sprint 9 step 3: tax group definition table + compliance guards.

Tests cover:
  TGD1: tax groups seeded in DB and readable
  TGD2: SELL with excise group without UKTZED → rejected
  TGD3: SELL with excise group without excise mark → rejected
  TGD4: SELL with excise group with UKTZED + excise mark → passes and crypto IS called
  TGD5: SELL with non-excise group without UKTZED → passes (not required)
  TGD6: no tax groups configured → validation skipped, passes
  TGD7: guard rejects before sign (no crypto call), error message names the item and group
  TGD8: both UKTZED and mark missing → error message mentions both
  TGD9: mixed goods: one excise (compliant) + one excise (non-compliant) → rejects with specific item
"""
from __future__ import annotations

from datetime import datetime, UTC

import pytest
from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, ShiftRepository, TaxGroupRepository
from prro_gateway.services.write_path import WritePathWorker


def _seed_tax_groups(conn, fiscal_number='FN-DEV-0001', excise_allowed=True):
    """Seed typical tax groups: А (ПДВ only), ГА (ПДВ + акциз)."""
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
        (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
         requires_uktzed, requires_excise_mark)
        VALUES (?, '1', 'А', 20.00, 0, 0, 0, 0, 0)
    """, (fiscal_number,))
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
        (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
         requires_uktzed, requires_excise_mark)
        VALUES (?, '4', 'ГА', 20.00, 5.00, 0, 2, 1, 1)
    """, (fiscal_number,))
    if excise_allowed:
        conn.execute("UPDATE node_state SET excise_allowed = 1 WHERE fiscal_number = ?", (fiscal_number,))
    conn.commit()


def _setup_shift(conn):
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = 'shift-tgd'").fetchone()
    if not existing:
        ShiftRepository.create_shift(
            conn, shift_id='shift-tgd', fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-14T08:00:00Z',
        )
        conn.commit()


def _enqueue(conn, rid, goods):
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
                'goods': goods,
                'payments': [{'amount': sum(g.get('sum', 0) for g in goods), 'type': 'CASH'}],
                'totals': {'total_sum': sum(g.get('sum', 0) for g in goods)},
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
    def sign_raw(self, *, data):
        self.call_count += 1
        return b'\x30\x82SIGNED'


class _OkTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(state='ACK', transport_request_id='tr',
                          submission_status='DPS_ACK', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


# ---------------------------------------------------------------------------
# TGD1 — tax groups seeded and readable
# ---------------------------------------------------------------------------

def test_tgd1_tax_groups_readable(conn) -> None:
    _seed_tax_groups(conn)
    groups = TaxGroupRepository.get_for_fiscal_number(conn, 'FN-DEV-0001')
    assert '1' in groups
    assert '4' in groups
    assert groups['1'].name == 'А'
    assert groups['4'].requires_uktzed is True
    assert groups['4'].requires_excise_mark is True
    assert groups['1'].requires_uktzed is False


# ---------------------------------------------------------------------------
# TGD2 — excise group without UKTZED → rejected
# ---------------------------------------------------------------------------

def test_tgd2_excise_without_uktzed_rejected(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd2', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'excise_barcodes': ['FRTG456985']},  # has mark, no uktzed
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert 'UKTZED' in result.canonical_error.message
    assert 'Горілка' in result.canonical_error.message, 'Error must name the specific item'
    assert 'ГА' in result.canonical_error.message, 'Error must name the tax group'


# ---------------------------------------------------------------------------
# TGD3 — excise group without excise mark → rejected
# ---------------------------------------------------------------------------

def test_tgd3_excise_without_mark_rejected(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd3', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900'},  # has uktzed, no mark
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert 'excise mark' in result.canonical_error.message
    assert 'Горілка' in result.canonical_error.message
    assert 'excise mark' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD4 — excise group with UKTZED + mark → passes
# ---------------------------------------------------------------------------

def test_tgd4_excise_with_all_fields_passes(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd4', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900',
         'excise_barcodes': ['FRTG456985']},
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Expected ACK, got {result.outcome}: {result.canonical_error}'
    assert crypto.call_count > 0, 'Crypto must be called — guard passed, signing happened'


# ---------------------------------------------------------------------------
# TGD5 — non-excise group without UKTZED → passes
# ---------------------------------------------------------------------------

def test_tgd5_non_excise_no_uktzed_passes(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd5', [
        {'name': 'Хліб', 'price': 3500, 'quantity': 1000, 'sum': 3500, 'tax_id': '1'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'


# ---------------------------------------------------------------------------
# TGD6 — no tax groups configured → skipped, passes
# ---------------------------------------------------------------------------

def test_tgd6_no_groups_configured_passes(conn) -> None:
    # Don't seed any tax groups
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd6', [
        {'name': 'Anything', 'price': 1000, 'quantity': 1000, 'sum': 1000, 'tax_id': '4'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'


# ---------------------------------------------------------------------------
# TGD7 — guard rejects BEFORE sign (no crypto call made)
# ---------------------------------------------------------------------------

def test_tgd7_guard_rejects_before_sign(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd7', [
        {'name': 'Горілка без марки', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4'},  # no uktzed, no mark
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert crypto.call_count == 0, 'Crypto must NOT be called when guard rejects'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    # Both UKTZED and mark are missing — error should mention both
    assert 'UKTZED' in result.canonical_error.message
    assert 'excise mark' in result.canonical_error.message
    assert 'Горілка без марки' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD8 — both missing → error mentions both violations
# ---------------------------------------------------------------------------

def test_tgd8_both_missing_error_mentions_both(conn) -> None:
    """When both UKTZED and excise mark are missing, error must list both."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd8', [
        {'name': 'Коньяк', 'price': 50000, 'quantity': 1000, 'sum': 50000, 'tax_id': '4'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    msg = result.canonical_error.message
    assert msg.count('Коньяк') == 2, f'Both violations must name the item. Got: {msg}'
    assert 'UKTZED' in msg
    assert 'excise mark' in msg


# ---------------------------------------------------------------------------
# TGD9 — mixed: compliant excise + non-compliant excise → rejects with specific item
# ---------------------------------------------------------------------------

def test_tgd9_mixed_compliant_and_non_compliant(conn) -> None:
    """Two excise items: one compliant, one not. Error must name only the bad one."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd9', [
        {'name': 'Горілка ОК', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900', 'excise_barcodes': ['FRTG456985']},
        {'name': 'Вино БЕЗ МАРКИ', 'price': 15000, 'quantity': 1000, 'sum': 15000,
         'tax_id': '4', 'uktzed': '2204101100'},  # has uktzed, NO mark
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    msg = result.canonical_error.message
    assert 'Вино БЕЗ МАРКИ' in msg, f'Error must name the non-compliant item. Got: {msg}'
    assert 'Горілка ОК' not in msg, f'Compliant item must NOT be in error. Got: {msg}'


# ---------------------------------------------------------------------------
# TGD10 — cash over 50 000 UAH → rejected
# ---------------------------------------------------------------------------

def test_tgd10_cash_over_limit_rejected(conn) -> None:
    """Cash payment > 50 000 UAH must be rejected before signing."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd10', [
        {'name': 'Дорогий товар', 'price': 5_500_000, 'quantity': 1000, 'sum': 5_500_000, 'tax_id': '1'},
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert '50 000' in result.canonical_error.message
    assert '55000.00' in result.canonical_error.message  # actual amount shown
    assert crypto.call_count == 0, 'Crypto must NOT be called'


# ---------------------------------------------------------------------------
# TGD11 — cash exactly 50 000 UAH → passes
# ---------------------------------------------------------------------------

def test_tgd11_cash_at_limit_rejected(conn) -> None:
    """50 000 UAH exact = rejected (>= limit, not just >)."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd11', [
        {'name': 'Товар на межі', 'price': 5_000_000, 'quantity': 1000, 'sum': 5_000_000, 'tax_id': '1'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert '50 000' in result.canonical_error.message


def test_tgd11b_cash_below_limit_passes(conn) -> None:
    """49 999.99 UAH = passes."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd11b', [
        {'name': 'Товар під межею', 'price': 4_999_999, 'quantity': 1000, 'sum': 4_999_999, 'tax_id': '1'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'49 999.99 UAH should pass. Got: {result.canonical_error}'


# ---------------------------------------------------------------------------
# TGD12 — cashless over 50 000 → passes (limit is cash only)
# ---------------------------------------------------------------------------

def test_tgd12_cashless_over_limit_passes(conn) -> None:
    """Cashless (card) payments have no 50k limit."""
    _seed_tax_groups(conn)
    conn.execute("INSERT OR IGNORE INTO payment_type_definitions (fiscal_number, type_code, type_group, name) VALUES ('FN-DEV-0001', 'CASHLESS', 2, 'Безготівковий')")
    conn.commit()
    _setup_shift(conn)
    # Payment type CASHLESS, not CASH
    cmd = CanonicalFiscalCommand(
        request_id='req-tgd12', idempotency_key='idem-tgd12',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-tgd12',
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Дуже дорогий', 'price': 10_000_000, 'quantity': 1000, 'sum': 10_000_000, 'tax_id': '1'}],
                'payments': [{'amount': 10_000_000, 'type': 'CASHLESS', 'payment_type': 'CASHLESS'}],
                'totals': {'total_sum': 10_000_000},
            },
        },
        payload_sha256='sha-tgd12',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-tgd12'),
        correlation_id='c-tgd12',
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
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Cashless over 50k should pass. Got: {result.canonical_error}'


# ---------------------------------------------------------------------------
# TGD13 — 3 units of vodka must have exactly 3 excise marks
# ---------------------------------------------------------------------------

def test_tgd13_mark_count_equals_quantity(conn) -> None:
    """3 bottles (Q=3000) with 3 marks → passes."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd13', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 3000, 'sum': 88680,
         'tax_id': '4', 'uktzed': '2208909900',
         'excise_barcodes': ['FRTG456985', 'FRTG456986', 'FRTG456987']},
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'3 units + 3 marks should pass. Got: {result.canonical_error}'
    assert crypto.call_count > 0


# ---------------------------------------------------------------------------
# TGD14 — 3 units but only 2 marks → rejected
# ---------------------------------------------------------------------------

def test_tgd14_fewer_marks_than_quantity_rejected(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd14', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 3000, 'sum': 88680,
         'tax_id': '4', 'uktzed': '2208909900',
         'excise_barcodes': ['FRTG456985', 'FRTG456986']},  # 2 marks, need 3
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'quantity=3' in result.canonical_error.message
    assert '2 excise marks' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD15 — 1 unit but 2 marks → rejected
# ---------------------------------------------------------------------------

def test_tgd15_more_marks_than_quantity_rejected(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd15', [
        {'name': 'Коньяк', 'price': 50000, 'quantity': 1000, 'sum': 50000,
         'tax_id': '4', 'uktzed': '2208201200',
         'excise_barcodes': ['ABCD123456', 'ABCD123457']},  # 2 marks, need 1
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'quantity=1' in result.canonical_error.message
    assert '2 excise marks' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD16 — fractional quantity with excise mark group → rejected
# ---------------------------------------------------------------------------

def test_tgd16_fractional_qty_excise_rejected(conn) -> None:
    """0.5 bottles (Q=500) with requires_excise_mark → rejected."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd16', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 500, 'sum': 14780,
         'tax_id': '4', 'uktzed': '2208909900',
         'excise_barcodes': ['FRTG456985']},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'fractional' in result.canonical_error.message
    assert '0.500' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD17 — fractional quantity WITHOUT excise mark group → passes (HoReCa)
# ---------------------------------------------------------------------------

def test_tgd17_fractional_qty_no_excise_passes(conn) -> None:
    """50ml vodka pour in HoReCa: TX=1 (no excise mark required) → passes."""
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd17', [
        {'name': 'Горілка 50мл', 'price': 5000, 'quantity': 50, 'sum': 250, 'tax_id': '1'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'HoReCa pour should pass. Got: {result.canonical_error}'


# ---------------------------------------------------------------------------
# TGD18 — adapter calculates sum = price * qty / 1000, ignores POS sum
# ---------------------------------------------------------------------------

def test_tgd18_adapter_calculates_sum() -> None:
    """Adapter must compute sum = price * quantity / 1000, not trust POS."""
    from prro_gateway.adapters.checkbox_rest import CheckboxRestAdapter

    adapter = CheckboxRestAdapter()
    cmd = adapter.map_command({
        'context': {
            'request_id': 'req-tgd18',
            'fiscal_number': 'FN-001',
            'business_ts': '2026-04-14T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [
                {'good': {'name': 'Товар'},
                 'price': 10000, 'quantity': 3000,
                 'sum': 99999},  # POS sends wrong sum — must be ignored
            ],
            'payments': [{'type': 'CASH', 'value': 30000}],
        },
    })
    goods = cmd.payload['receipt']['goods']
    assert goods[0]['sum'] == 30000, f'sum must be 10000*3000/1000=30000, not POS value. Got: {goods[0]["sum"]}'


# ---------------------------------------------------------------------------
# TGD19 — empty goods → rejected
# ---------------------------------------------------------------------------

def test_tgd19_empty_goods_rejected(conn) -> None:
    _setup_shift(conn)
    cmd = CanonicalFiscalCommand(
        request_id='req-tgd19', idempotency_key='idem-tgd19',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-tgd19',
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL', 'goods': [], 'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 0}}},
        payload_sha256='sha-tgd19',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-tgd19'),
        correlation_id='c-tgd19',
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
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'no goods' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD20 — no payments → rejected
# ---------------------------------------------------------------------------

def test_tgd20_no_payments_rejected(conn) -> None:
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd20', [
        {'name': 'Товар', 'price': 1000, 'quantity': 1000, 'sum': 1000, 'tax_id': '1'},
    ])
    # Patch the inbox payload to have empty payments
    conn.execute("UPDATE ingress_inbox SET payload_json = REPLACE(payload_json, '\"payments\":[{\"amount\":1000,\"type\":\"CASH\"}]', '\"payments\":[]') WHERE request_id = 'req-tgd20'")
    conn.commit()
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'no payments' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD21 — negative price → rejected
# ---------------------------------------------------------------------------

def test_tgd21_negative_price_rejected(conn) -> None:
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd21', [
        {'name': 'Мінус товар', 'price': -500, 'quantity': 1000, 'sum': -500, 'tax_id': '1'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'negative' in result.canonical_error.message
    assert 'Мінус товар' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD22 — invalid excise mark format → rejected
# ---------------------------------------------------------------------------

def test_tgd22_invalid_excise_mark_format_rejected(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd22', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900',
         'excise_barcodes': ['AB']},  # too short
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'invalid excise mark format' in result.canonical_error.message
    assert 'AB' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD23 — empty string excise mark → rejected
# ---------------------------------------------------------------------------

def test_tgd23_empty_excise_mark_rejected(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd23', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900',
         'excise_barcodes': ['']},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'invalid excise mark format' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD24 — valid excise mark format → passes
# ---------------------------------------------------------------------------

def test_tgd24_valid_excise_mark_passes(conn) -> None:
    _seed_tax_groups(conn)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd24', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900',
         'excise_barcodes': ['FRTG456985']},
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Valid mark should pass. Got: {result.canonical_error}'
    assert crypto.call_count > 0


# ---------------------------------------------------------------------------
# TGD25 — excise_allowed=0: excise goods rejected (master switch)
# ---------------------------------------------------------------------------

def test_tgd25_excise_not_allowed_rejected(conn) -> None:
    """PRRO without excise license: any excise goods blocked."""
    _seed_tax_groups(conn, excise_allowed=False)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd25', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900', 'excise_barcodes': ['FRTG456985']},
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'not allowed' in result.canonical_error.message
    assert 'Горілка' in result.canonical_error.message
    assert crypto.call_count == 0, 'Crypto must NOT be called'


# ---------------------------------------------------------------------------
# TGD26 — excise_allowed=0: non-excise goods pass
# ---------------------------------------------------------------------------

def test_tgd26_excise_not_allowed_non_excise_passes(conn) -> None:
    """PRRO without excise license: regular goods still work."""
    _seed_tax_groups(conn, excise_allowed=False)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd26', [
        {'name': 'Хліб', 'price': 3500, 'quantity': 1000, 'sum': 3500, 'tax_id': '1'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Regular goods should pass. Got: {result.canonical_error}'


# ---------------------------------------------------------------------------
# TGD27 — excise_allowed=1: excise goods pass (with all compliance fields)
# ---------------------------------------------------------------------------

def test_tgd27_excise_allowed_passes(conn) -> None:
    """PRRO with excise license + all compliance fields → passes."""
    _seed_tax_groups(conn, excise_allowed=True)
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd27', [
        {'name': 'Горілка', 'price': 29560, 'quantity': 1000, 'sum': 29560,
         'tax_id': '4', 'uktzed': '2208909900', 'excise_barcodes': ['FRTG456985']},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Excise allowed + compliant should pass. Got: {result.canonical_error}'


# ---------------------------------------------------------------------------
# TGD28 — unknown tax group (configured groups exist but this one missing) → rejected
# ---------------------------------------------------------------------------

def test_tgd28_unknown_group_rejected(conn) -> None:
    """Tax group TX=99 not configured for this FN → rejected with clear message."""
    _seed_tax_groups(conn, excise_allowed=True)  # seeds groups 1 and 4
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd28', [
        {'name': 'Невідомий товар', 'price': 1000, 'quantity': 1000, 'sum': 1000, 'tax_id': '99'},
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'TX=99' in result.canonical_error.message
    assert 'not configured' in result.canonical_error.message
    assert 'Невідомий товар' in result.canonical_error.message
    assert crypto.call_count == 0


# ---------------------------------------------------------------------------
# TGD29 — business_ts in future → rejected
# ---------------------------------------------------------------------------

def test_tgd29_future_timestamp_rejected(conn) -> None:
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    _setup_shift(conn)
    future_ts = datetime.now(UTC) + __import__('datetime').timedelta(hours=2)
    cmd = CanonicalFiscalCommand(
        request_id='req-tgd29', idempotency_key='idem-tgd29',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-tgd29',
        business_ts=future_ts,
        payload={'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                             'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}}},
        payload_sha256='sha-tgd29',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-tgd29'),
        correlation_id='c-tgd29',
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
    from prro_gateway.enums import CanonicalErrorCode as CEC
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN',
                             validate_timestamps=True)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CEC.INVALID_FISCAL_DATE
    assert 'future' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD30 — empty product name → rejected
# ---------------------------------------------------------------------------

def test_tgd30_empty_product_name_rejected(conn) -> None:
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd30', [
        {'name': '', 'price': 100, 'quantity': 1000, 'sum': 100, 'tax_id': '1'},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert 'empty product name' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD31 — shift > 24h → fiscal ops blocked
# ---------------------------------------------------------------------------

def test_tgd31_shift_over_24h_blocked(conn) -> None:
    old_time = (datetime.now(UTC) - __import__('datetime').timedelta(hours=25)).isoformat()
    ShiftRepository.create_shift(
        conn, shift_id='shift-tgd31', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at=old_time,
    )
    conn.execute("UPDATE shifts SET opened_at = ?, created_at = ? WHERE shift_id = 'shift-tgd31'",
                 (old_time, old_time))
    conn.commit()
    _enqueue(conn, 'req-tgd31', [
        {'name': 'Товар', 'price': 100, 'quantity': 1000, 'sum': 100},
    ])
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert '24h' in result.canonical_error.message or '24' in result.canonical_error.message
    assert 'Z_REPORT' in result.canonical_error.message


# ---------------------------------------------------------------------------
# TGD30 — TXAL=3 (absolute excise) → rejected before sign
# ---------------------------------------------------------------------------

def test_tgd30_txal3_rejected(conn) -> None:
    """Tax group with tax_algorithm=3 (absolute excise per quantity) must be
    rejected before crypto/transport — not silently produce zero excise."""
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
        (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
         requires_uktzed, requires_excise_mark)
        VALUES ('FN-DEV-0001', '7', 'Паливо', 20.00, 0, 0, 3, 0, 0)
    """)
    conn.execute("UPDATE node_state SET excise_allowed = 1 WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()
    _setup_shift(conn)
    _enqueue(conn, 'req-tgd29', [
        {'name': 'Бензин А-95', 'price': 5499, 'quantity': 20000, 'sum': 109980, 'tax_id': '7'},
    ])
    crypto = _Crypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert 'TXAL=3' in result.canonical_error.message
    assert 'Бензин А-95' in result.canonical_error.message
    assert 'not supported' in result.canonical_error.message
    assert crypto.call_count == 0, 'Crypto must NOT be called'
