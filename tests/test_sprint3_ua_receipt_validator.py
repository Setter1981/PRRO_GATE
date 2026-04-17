"""
Sprint 3 / step 1 — Ukrainian receipt validator foundation tests.

Coverage matrix:
  V1 — valid SELL passes validator
  V2 — valid RETURN passes validator
  V3 — empty goods rejected
  V4 — empty payments rejected
  V5 — goods total mismatch rejected
  V6 — payments sum mismatch rejected
  V7 — line sum inconsistency rejected
  V8 — invalid payload rejected before transport (end-to-end)
"""
from __future__ import annotations

from prro_gateway.validators.ua_receipt import validate_sell_return_receipt


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _valid_sell_payload(**overrides) -> dict:
    receipt = {
        'type': 'SELL',
        'goods': [
            {'name': 'Coffee', 'price': 5000, 'quantity': 1000, 'sum': 5000},
            {'name': 'Sugar', 'price': 1500, 'quantity': 2000, 'sum': 3000},
        ],
        'payments': [{'amount': 8000, 'type': 'CASH'}],
        'totals': {'total_sum': 8000},
    }
    receipt.update(overrides)
    return {'receipt': receipt}


def _valid_return_payload(**overrides) -> dict:
    receipt = {
        'type': 'RETURN',
        'goods': [
            {'name': 'Coffee', 'price': 5000, 'quantity': 1000, 'sum': 5000},
        ],
        'payments': [{'amount': 5000, 'type': 'CASH'}],
        'totals': {'total_sum': 5000},
        'related_receipt_id': 'rcpt-001',
    }
    receipt.update(overrides)
    return {'receipt': receipt}


# ---------------------------------------------------------------------------
# V1 — valid SELL passes
# ---------------------------------------------------------------------------

def test_v1_valid_sell_passes() -> None:
    errors = validate_sell_return_receipt(_valid_sell_payload())
    assert errors == [], f'Valid SELL must pass, got: {errors}'


# ---------------------------------------------------------------------------
# V2 — valid RETURN passes
# ---------------------------------------------------------------------------

def test_v2_valid_return_passes() -> None:
    errors = validate_sell_return_receipt(_valid_return_payload())
    assert errors == [], f'Valid RETURN must pass, got: {errors}'


# ---------------------------------------------------------------------------
# V3 — empty goods rejected
# ---------------------------------------------------------------------------

def test_v3_empty_goods_rejected() -> None:
    payload = _valid_sell_payload(goods=[])
    errors = validate_sell_return_receipt(payload)
    assert any('goods' in e for e in errors), f'Empty goods must fail: {errors}'


def test_v3b_missing_goods_rejected() -> None:
    payload = _valid_sell_payload()
    del payload['receipt']['goods']
    errors = validate_sell_return_receipt(payload)
    assert any('goods' in e for e in errors), f'Missing goods must fail: {errors}'


# ---------------------------------------------------------------------------
# V4 — empty payments rejected
# ---------------------------------------------------------------------------

def test_v4_empty_payments_rejected() -> None:
    payload = _valid_sell_payload(payments=[])
    errors = validate_sell_return_receipt(payload)
    assert any('payments' in e for e in errors), f'Empty payments must fail: {errors}'


# ---------------------------------------------------------------------------
# V5 — goods total mismatch
# ---------------------------------------------------------------------------

def test_v5_goods_total_mismatch() -> None:
    payload = _valid_sell_payload(
        totals={'total_sum': 9999},  # goods sum = 8000
    )
    errors = validate_sell_return_receipt(payload)
    assert any('total_sum' in e and 'goods' in e for e in errors), (
        f'Goods total mismatch must fail: {errors}'
    )


# ---------------------------------------------------------------------------
# V6 — payments sum mismatch
# ---------------------------------------------------------------------------

def test_v6_payments_sum_mismatch() -> None:
    payload = _valid_sell_payload(
        payments=[{'amount': 7000, 'type': 'CASH'}],
        # total_sum=8000, payment=7000
    )
    errors = validate_sell_return_receipt(payload)
    assert any('total_sum' in e and 'payments' in e for e in errors), (
        f'Payment sum mismatch must fail: {errors}'
    )


def test_v6b_payments_goods_mismatch_without_totals() -> None:
    """When totals is omitted, payments must equal goods sum."""
    payload = {
        'receipt': {
            'type': 'SELL',
            'goods': [{'name': 'A', 'price': 5000, 'quantity': 1000, 'sum': 5000}],
            'payments': [{'amount': 3000, 'type': 'CASH'}],
            # no totals
        },
    }
    errors = validate_sell_return_receipt(payload)
    assert any('underpayment' in e for e in errors), (
        f'Goods/payments mismatch without totals must fail: {errors}'
    )


# ---------------------------------------------------------------------------
# V7 — line sum inconsistency
# ---------------------------------------------------------------------------

def test_v7_line_sum_inconsistency() -> None:
    payload = _valid_sell_payload(
        goods=[{'name': 'Bad', 'price': 1000, 'quantity': 2000, 'sum': 9999}],
        payments=[{'amount': 9999, 'type': 'CASH'}],
        totals={'total_sum': 9999},
    )
    errors = validate_sell_return_receipt(payload)
    assert any('price' in e and 'quantity' in e for e in errors), (
        f'Line sum inconsistency must fail: {errors}'
    )


# ---------------------------------------------------------------------------
# V8 — invalid payload rejected before transport (end-to-end via write-path)
# ---------------------------------------------------------------------------

def test_v8_invalid_payload_rejected_before_transport(conn) -> None:
    """Write-path must reject SELL with invalid receipt data before transport."""
    import sqlite3
    from datetime import datetime, UTC
    from prro_gateway.enums import CanonicalErrorCode, DocumentState, OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import FiscalDocumentRepository, InboxRepository, ShiftRepository

    # Create shift
    ShiftRepository.create_shift(
        conn,
        shift_id='shift-v8',
        fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED,
        open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST,
        integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    # Build bad command: goods/payments sum mismatch
    cmd = CanonicalFiscalCommand(
        request_id='req-v8',
        idempotency_key='idem-v8',
        protocol=Protocol.CHECKBOX_REST,
        operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001',
        route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='test',
        external_request_id='ext-v8',
        business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': [{'amount': 500, 'type': 'CASH'}],  # underpayment — invalid
                'totals': {'total_sum': 1000},
            },
        },
        payload_sha256='sha-v8',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1234, session_id='s1', correlation_id='c-v8'),
        correlation_id='c-v8',
    )

    # Store in inbox
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn,
        request_id=cmd.request_id,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number,
        route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner,
        external_request_id=cmd.external_request_id,
        protocol_session_id=None,
        payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256,
        response_deadline_at='2026-04-12T12:01:00Z',
    )
    conn.commit()

    # Process
    from prro_gateway.services.write_path import WritePathWorker

    class _NoTransport:
        def send(self, **kw):
            raise AssertionError('Transport must not be called for invalid receipt')

    class _NoCrypto:
        def sign(self, **kw):
            raise AssertionError('Crypto must not be called for invalid receipt')

    worker = WritePathWorker(crypto_provider=_NoCrypto(), transport_client=_NoTransport())
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ERROR'
    assert result.canonical_error is not None
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert 'payments' in result.canonical_error.message
