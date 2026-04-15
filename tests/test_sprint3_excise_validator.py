"""
Sprint 3 / step 3 — excise validation foundation tests.

Coverage matrix:
  EX1 — valid excise item passes (uktzed + valid barcodes)
  EX2 — item without excise_barcodes passes (no excise rules apply)
  EX3 — excise mark without uktzed rejected
  EX4 — malformed excise mark (normalizes to empty) rejected
  EX5 — null excise mark entry rejected
  EX6 — intra-receipt duplicate mark rejected
  EX7 — duplicate mark still blocked at DB level (existing protection intact)
  EX8 — invalid excise payload rejected before sign/send (end-to-end)
"""
from __future__ import annotations

from prro_gateway.validators.ua_receipt import validate_excise_marks


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# Helpers
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def _payload_with_goods(*goods_items) -> dict:
    return {
        'receipt': {
            'type': 'SELL',
            'goods': list(goods_items),
            'payments': [{'amount': 5000, 'type': 'CASH'}],
            'totals': {'total_sum': 5000},
        },
    }


def _excise_good(*, name='Alcohol', uktzed='2208', barcodes=None, **kw) -> dict:
    item = {'name': name, 'uktzed': uktzed, 'excise_barcodes': barcodes or [], 'price': 5000, 'quantity': 1000, 'sum': 5000, **kw}
    return item


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# EX1 — valid excise item passes
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_ex1_valid_excise_item_passes() -> None:
    payload = _payload_with_goods(_excise_good(barcodes=['AB-123', 'CD-456']))
    assert validate_excise_marks(payload) == []


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# EX2 — item without excise_barcodes passes
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_ex2_no_excise_barcodes_passes() -> None:
    payload = _payload_with_goods({'name': 'Coffee', 'price': 1000, 'quantity': 1000, 'sum': 1000})
    assert validate_excise_marks(payload) == []


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# EX3 — excise mark without uktzed rejected
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_ex3_excise_without_uktzed_rejected() -> None:
    payload = _payload_with_goods(_excise_good(uktzed=None, barcodes=['AB-123']))
    errors = validate_excise_marks(payload)
    assert any('uktzed' in e for e in errors), f'Missing uktzed must fail: {errors}'


def test_ex3b_excise_with_empty_uktzed_rejected() -> None:
    payload = _payload_with_goods(_excise_good(uktzed='', barcodes=['AB-123']))
    errors = validate_excise_marks(payload)
    assert any('uktzed' in e for e in errors), f'Empty uktzed must fail: {errors}'


# ---------------------------------------------------------------------------
# EX4 — malformed excise mark rejected
# ---------------------------------------------------------------------------

def test_ex4_malformed_mark_rejected() -> None:
    payload = _payload_with_goods(_excise_good(barcodes=['']))
    errors = validate_excise_marks(payload)
    assert any('empty' in e.lower() for e in errors), f'Malformed mark must fail: {errors}'


def test_ex4b_whitespace_only_mark_rejected() -> None:
    payload = _payload_with_goods(_excise_good(barcodes=['   ']))
    errors = validate_excise_marks(payload)
    assert any('empty string' in e for e in errors), f'Whitespace mark must fail: {errors}'


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# EX5 — null excise mark entry rejected
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_ex5_null_mark_entry_rejected() -> None:
    payload = _payload_with_goods(_excise_good(barcodes=[None]))
    errors = validate_excise_marks(payload)
    assert any('invalid' in e.lower() or 'null' in e.lower() for e in errors), (
        f'Null mark must fail: {errors}'
    )


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# EX6 — intra-receipt duplicate mark rejected
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_ex6_intra_receipt_duplicate_rejected() -> None:
    payload = _payload_with_goods(
        _excise_good(name='Bottle A', barcodes=['AB-123']),
        _excise_good(name='Bottle B', barcodes=['AB-123']),  # same mark
    )
    errors = validate_excise_marks(payload)
    assert any('duplicate' in e.lower() for e in errors), (
        f'Duplicate mark within receipt must fail: {errors}'
    )


def test_ex6b_same_mark_different_case_rejected() -> None:
    """abcd000123 and ABCD000123 normalize to the same mark."""
    payload = _payload_with_goods(
        _excise_good(name='Bottle A', barcodes=['abcd000123']),
        _excise_good(name='Bottle B', barcodes=['ABCD000123']),
    )
    errors = validate_excise_marks(payload)
    assert any('duplicate' in e.lower() for e in errors), (
        f'Case-insensitive duplicate must fail: {errors}'
    )


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# EX7 — duplicate mark at DB level (existing protection intact)
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_ex7_duplicate_mark_db_level(conn) -> None:
    """Existing DuplicateExciseMarkError is still enforced at DB level."""
    from prro_gateway.enums import CanonicalErrorCode
    from prro_gateway.services.write_path import WritePathWorker

    # Use the existing test pattern from test_write_path.py
    from prro_gateway.enums import OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from datetime import datetime, UTC

    ShiftRepository.create_shift(
        conn, shift_id='shift-ex7', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    def _make_cmd(req_id):
        return CanonicalFiscalCommand(
            request_id=req_id, idempotency_key=f'idem-{req_id}',
            protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
            fiscal_number='FN-DEV-0001', route_key='main',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            channel_owner='test', external_request_id=f'ext-{req_id}',
            business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
            payload={
                'receipt': {
                    'type': 'SELL',
                    'goods': [{'name': 'Wine', 'uktzed': '2204', 'excise_barcodes': ['XEDB000001'], 'price': 5000, 'quantity': 1000, 'sum': 5000}],
                    'payments': [{'amount': 5000, 'type': 'CASH'}],
                    'totals': {'total_sum': 5000},
                },
            },
            payload_sha256=f'sha-{req_id}',
            trace_context=TraceContext(source_ip='10.0.0.1', source_port=1234, session_id='s1', correlation_id=f'c-{req_id}'),
            correlation_id=f'c-{req_id}',
        )

    for rid in ('req-ex7a', 'req-ex7b'):
        cmd = _make_cmd(rid)
        conn.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
            protocol=cmd.protocol, operation_type=cmd.operation_type,
            fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
            backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
            channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
            protocol_session_id=None, payload_json=cmd.model_dump_json(),
            payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-12T12:01:00Z',
        )
        conn.commit()

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-ex7', submission_status='DONE',
                              server_fiscal_no='SFN-EX7', response_json='{}',
                              sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    class _StubCrypto:
        def sign(self, *, payload_json, **kw):
            return f'signed::{payload_json}'

    worker = WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_StubTransport())
    first = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert first.outcome == 'ACK', f'First sell must ACK: {first.outcome}'

    second = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert second.outcome == 'ERROR'
    assert second.canonical_error.code == CanonicalErrorCode.DUPLICATE_EXCISE_MARK


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# EX8 — invalid excise rejected before sign/send (end-to-end)
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_ex8_invalid_excise_rejected_before_transport(conn) -> None:
    """SELL with excise mark but no uktzed must be rejected before sign/send."""
    from datetime import datetime, UTC
    from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-ex8', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-ex8', idempotency_key='idem-ex8',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='test', external_request_id='ext-ex8',
        business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Wine', 'excise_barcodes': ['XEMK000001'], 'price': 5000, 'quantity': 1000, 'sum': 5000}],
                'payments': [{'amount': 5000, 'type': 'CASH'}],
                'totals': {'total_sum': 5000},
            },
        },
        payload_sha256='sha-ex8',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1234, session_id='s1', correlation_id='c-ex8'),
        correlation_id='c-ex8',
    )

    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-12T12:01:00Z',
    )
    conn.commit()

    class _NoTransport:
        def send(self, **kw): raise AssertionError('Transport must not be called')
    class _NoCrypto:
        def sign(self, **kw): raise AssertionError('Crypto must not be called')

    worker = WritePathWorker(crypto_provider=_NoCrypto(), transport_client=_NoTransport())
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert 'uktzed' in result.canonical_error.message
