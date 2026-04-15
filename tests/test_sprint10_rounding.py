"""Sprint 10 step 7: Rounding (SMP) per Постанова НБУ №115.

Tests:
  RND1: round up to .50 (remainder 25-49)
  RND2: round up to .00 (remainder 75-99)
  RND3: round down to .00 (remainder 01-24)
  RND4: rounding_enabled=0 → no SMP
  RND5: cashless → no SMP
  RND6: exact boundary → no SMP
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker
from prro_gateway.utils.rounding import round_to_50_kopecks


# ---------------------------------------------------------------------------
# Unit tests for rounding function
# ---------------------------------------------------------------------------

def test_round_function_boundaries() -> None:
    assert round_to_50_kopecks(12500) == 12500   # .00 → no change
    assert round_to_50_kopecks(12550) == 12550   # .50 → no change
    assert round_to_50_kopecks(12513) == 12500   # .13 → ↓ .00
    assert round_to_50_kopecks(12524) == 12500   # .24 → ↓ .00
    assert round_to_50_kopecks(12525) == 12550   # .25 → ↑ .50
    assert round_to_50_kopecks(12549) == 12550   # .49 → ↑ .50
    assert round_to_50_kopecks(12551) == 12550   # .51 → ↓ .50
    assert round_to_50_kopecks(12574) == 12550   # .74 → ↓ .50
    assert round_to_50_kopecks(12575) == 12600   # .75 → ↑ .00
    assert round_to_50_kopecks(12599) == 12600   # .99 → ↑ .00


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _setup(conn, rounding_enabled=1):
    conn.execute(
        "UPDATE node_state SET rounding_enabled = ? WHERE fiscal_number = 'FN-DEV-0001'",
        (rounding_enabled,),
    )
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = 'shift-rnd'").fetchone()
    if not existing:
        ShiftRepository.create_shift(
            conn, shift_id='shift-rnd', fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-14T08:00:00Z',
        )
    conn.commit()


def _enqueue_sell(conn, rid, total_sum, payments):
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
# RND1 — round up to .50
# ---------------------------------------------------------------------------

def test_rnd1_round_up_to_50(conn) -> None:
    _setup(conn, rounding_enabled=1)
    # 125.25 → 125.50, SMP = +25
    _enqueue_sell(conn, 'req-rnd1', 12525, [{'amount': 12525, 'type': 'CASH'}])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP="25"' in xml, f'SMP must be 25. Got: {xml}'


# ---------------------------------------------------------------------------
# RND2 — round up to .00
# ---------------------------------------------------------------------------

def test_rnd2_round_up_to_00(conn) -> None:
    _setup(conn, rounding_enabled=1)
    # 125.75 → 126.00, SMP = +25
    _enqueue_sell(conn, 'req-rnd2', 12575, [{'amount': 12575, 'type': 'CASH'}])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP="25"' in xml, f'SMP must be 25. Got: {xml}'


# ---------------------------------------------------------------------------
# RND3 — round down to .00
# ---------------------------------------------------------------------------

def test_rnd3_round_down_to_00(conn) -> None:
    _setup(conn, rounding_enabled=1)
    # 125.13 → 125.00, SMP = -13
    _enqueue_sell(conn, 'req-rnd3', 12513, [{'amount': 12513, 'type': 'CASH'}])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP="-13"' in xml, f'SMP must be -13. Got: {xml}'


# ---------------------------------------------------------------------------
# RND4 — rounding_enabled=0 → no SMP
# ---------------------------------------------------------------------------

def test_rnd4_disabled_no_smp(conn) -> None:
    _setup(conn, rounding_enabled=0)
    _enqueue_sell(conn, 'req-rnd4', 12525, [{'amount': 12525, 'type': 'CASH'}])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP=' not in xml, f'SMP must be absent when disabled. Got: {xml}'


# ---------------------------------------------------------------------------
# RND5 — cashless → no SMP
# ---------------------------------------------------------------------------

def test_rnd5_cashless_no_smp(conn) -> None:
    _setup(conn, rounding_enabled=1)
    conn.execute("INSERT OR IGNORE INTO payment_type_definitions (fiscal_number, type_code, type_group, name) VALUES ('FN-DEV-0001', 'CARD', 2, 'Картка')")
    conn.commit()
    _enqueue_sell(conn, 'req-rnd5', 12525, [{'amount': 12525, 'type': 'CARD'}])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP=' not in xml, f'SMP must be absent for cashless. Got: {xml}'


# ---------------------------------------------------------------------------
# RND6 — exact boundary → no SMP
# ---------------------------------------------------------------------------

def test_rnd6_exact_boundary_no_smp(conn) -> None:
    _setup(conn, rounding_enabled=1)
    _enqueue_sell(conn, 'req-rnd6', 12500, [{'amount': 12500, 'type': 'CASH'}])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP=' not in xml, f'SMP must be absent for exact boundary. Got: {xml}'


# ---------------------------------------------------------------------------
# RND7 — RETURN with rounding
# ---------------------------------------------------------------------------

def test_rnd7_return_with_rounding(conn) -> None:
    _setup(conn, rounding_enabled=1)
    # Enqueue RETURN instead of SELL
    cmd = CanonicalFiscalCommand(
        request_id='req-rnd7', idempotency_key='idem-rnd7',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.RETURN,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-rnd7',
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'RETURN',
                'goods': [{'name': 'Item', 'price': 12525, 'quantity': 1000, 'sum': 12525}],
                'payments': [{'amount': 12525, 'type': 'CASH'}],
                'totals': {'total_sum': 12525},
                'related_receipt_id': 'orig-001',
            },
        },
        payload_sha256='sha-rnd7',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-rnd7'),
        correlation_id='c-rnd7',
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

    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN',
                             require_return_linkage=False)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP="25"' in xml, f'RETURN must also get SMP. Got: {xml}'


# ---------------------------------------------------------------------------
# RND8 — mixed CASH+CARD with rounding
# ---------------------------------------------------------------------------

def test_rnd8_mixed_payment_with_rounding(conn) -> None:
    _setup(conn, rounding_enabled=1)
    conn.execute("INSERT OR IGNORE INTO payment_type_definitions (fiscal_number, type_code, type_group, name) VALUES ('FN-DEV-0001', 'VISA', 2, 'VISA')")
    conn.commit()
    # total=12525, CASH=8000 + VISA=4525
    _enqueue_sell(conn, 'req-rnd8', 12525, [
        {'amount': 8000, 'type': 'CASH'},
        {'amount': 4525, 'type': 'VISA'},
    ])
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = crypto.signed[0]
    assert 'SMP="25"' in xml, f'Mixed with cash must have SMP. Got: {xml}'
    # SMP only on cash M, not on VISA M
    import re
    m_tags = re.findall(r'<M [^>]+>', xml)
    visa_ms = [m for m in m_tags if 'VISA' in m]
    assert len(visa_ms) == 1 and 'SMP=' not in visa_ms[0], f'VISA M must NOT have SMP. Got: {visa_ms}'
