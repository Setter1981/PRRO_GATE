"""
Sprint 3 / step 2 — validator expansion for SERVICE_IN / SERVICE_OUT / CASH_WITHDRAWAL.

Coverage matrix:
  SV1 — valid SERVICE_IN passes
  SV2 — valid SERVICE_OUT passes
  SV3 — missing service_sum rejected
  SV4 — zero service_sum rejected
  SV5 — service payments mismatch rejected (when payments present)
  CW1 — valid CASH_WITHDRAWAL passes
  CW2 — missing cash_withdrawal_sum rejected
  CW3 — empty payments rejected for CASH_WITHDRAWAL
  CW4 — payments/cash_withdrawal_sum mismatch rejected
  CW5 — totals/cash_withdrawal_sum mismatch rejected
  E2E — invalid SERVICE_IN rejected before transport (end-to-end via write-path)
"""
from __future__ import annotations

from prro_gateway.validators.ua_receipt import validate_service_receipt, validate_cash_withdrawal_receipt


# ---------------------------------------------------------------------------
# SERVICE_IN / SERVICE_OUT
# ---------------------------------------------------------------------------

def test_sv1_valid_service_in_passes() -> None:
    payload = {'service_sum': 12000, 'receipt': {'type': 'SERVICE_IN', 'goods': [], 'payments': []}}
    assert validate_service_receipt(payload) == []


def test_sv2_valid_service_out_passes() -> None:
    payload = {'service_sum': 8000, 'receipt': {'type': 'SERVICE_OUT', 'goods': [], 'payments': []}}
    assert validate_service_receipt(payload) == []


def test_sv3_missing_service_sum_rejected() -> None:
    payload = {'receipt': {'type': 'SERVICE_IN'}}
    errors = validate_service_receipt(payload)
    assert any('service_sum' in e for e in errors)


def test_sv3b_missing_receipt_rejected() -> None:
    """SERVICE_IN with valid service_sum but no receipt must be rejected."""
    payload = {'service_sum': 12000}
    errors = validate_service_receipt(payload)
    assert any('receipt' in e for e in errors), f'Missing receipt must fail: {errors}'


def test_sv4_zero_service_sum_rejected() -> None:
    payload = {'service_sum': 0, 'receipt': {'type': 'SERVICE_IN'}}
    errors = validate_service_receipt(payload)
    assert any('service_sum' in e for e in errors)


def test_sv5_service_payments_mismatch_rejected() -> None:
    """When payments are present, sum must equal service_sum."""
    payload = {
        'service_sum': 12000,
        'receipt': {
            'type': 'SERVICE_IN',
            'payments': [{'amount': 5000, 'type': 'CASH'}],
        },
    }
    errors = validate_service_receipt(payload)
    assert any('payments' in e and 'service_sum' in e for e in errors)


def test_sv5b_service_payments_match_passes() -> None:
    """When payments are present and match service_sum, validation passes."""
    payload = {
        'service_sum': 12000,
        'receipt': {
            'type': 'SERVICE_IN',
            'payments': [{'amount': 12000, 'type': 'CASH'}],
        },
    }
    assert validate_service_receipt(payload) == []


# ---------------------------------------------------------------------------
# CASH_WITHDRAWAL
# ---------------------------------------------------------------------------

def test_cw1_valid_cash_withdrawal_passes() -> None:
    payload = {
        'cash_withdrawal_sum': 7000,
        'receipt': {
            'type': 'CASH_WITHDRAWAL',
            'payments': [{'amount': 7000, 'type': 'CASHLESS'}],
            'totals': {'total_sum': 7000},
        },
    }
    assert validate_cash_withdrawal_receipt(payload) == []


def test_cw2_missing_cash_withdrawal_sum_rejected() -> None:
    payload = {
        'receipt': {
            'type': 'CASH_WITHDRAWAL',
            'payments': [{'amount': 7000, 'type': 'CASHLESS'}],
        },
    }
    errors = validate_cash_withdrawal_receipt(payload)
    assert any('cash_withdrawal_sum' in e for e in errors)


def test_cw3_empty_payments_rejected() -> None:
    payload = {
        'cash_withdrawal_sum': 7000,
        'receipt': {
            'type': 'CASH_WITHDRAWAL',
            'payments': [],
            'totals': {'total_sum': 7000},
        },
    }
    errors = validate_cash_withdrawal_receipt(payload)
    assert any('payments' in e for e in errors)


def test_cw4_payments_sum_mismatch_rejected() -> None:
    payload = {
        'cash_withdrawal_sum': 7000,
        'receipt': {
            'type': 'CASH_WITHDRAWAL',
            'payments': [{'amount': 3000, 'type': 'CASHLESS'}],
            'totals': {'total_sum': 7000},
        },
    }
    errors = validate_cash_withdrawal_receipt(payload)
    assert any('payments' in e and 'cash_withdrawal_sum' in e for e in errors)


def test_cw5_totals_mismatch_rejected() -> None:
    payload = {
        'cash_withdrawal_sum': 7000,
        'receipt': {
            'type': 'CASH_WITHDRAWAL',
            'payments': [{'amount': 7000, 'type': 'CASHLESS'}],
            'totals': {'total_sum': 9999},
        },
    }
    errors = validate_cash_withdrawal_receipt(payload)
    assert any('total_sum' in e and 'cash_withdrawal_sum' in e for e in errors)


# ---------------------------------------------------------------------------
# E2E — invalid SERVICE_IN rejected before transport
# ---------------------------------------------------------------------------

def test_e2e_invalid_service_rejected_before_transport(conn) -> None:
    """Write-path must reject SERVICE_IN with missing service_sum."""
    from datetime import datetime, UTC
    from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-sv-e2e', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-sv-e2e', idempotency_key='idem-sv-e2e',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SERVICE_IN,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='test', external_request_id='ext-sv-e2e',
        business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SERVICE_IN'}},  # missing service_sum
        payload_sha256='sha-sv-e2e',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1234, session_id='s1', correlation_id='c-sv'),
        correlation_id='c-sv',
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
    assert result.canonical_error is not None
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert 'service_sum' in result.canonical_error.message
