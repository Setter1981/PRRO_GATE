"""
G2 — Cash balance carry-over across shifts.

Full cross-shift sequence: Z_REPORT in Shift 1 writes last_cash_balance →
Shift 2 SHIFT_OPEN DPS XML carries SM=<balance>.

Coverage:
  CVO1: preserve mode — Z_REPORT ACK writes balance → SHIFT_OPEN XML SM=balance
  CVO2: reset mode — Z_REPORT ACK writes 0 → SHIFT_OPEN XML SM="0"
  CVO3: zero balance after empty shift → SHIFT_OPEN XML SM="0"

What's new vs existing tests (CB4, CB5, CB6):
  CB6 verifies Z_REPORT writes last_cash_balance.
  CB4/CB5 verify SHIFT_OPEN SM from pre-set last_cash_balance.
  G2 combines: Z_REPORT through write-path → last_cash_balance → SHIFT_OPEN XML,
  proving the full round-trip across a shift boundary.
"""
from __future__ import annotations

import json
from datetime import datetime, UTC

from prro_gateway.enums import OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, NodeStateRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


# ---------------------------------------------------------------------------
# Stubs
# ---------------------------------------------------------------------------

class _SpyCrypto:
    """Captures all payloads passed to sign()."""
    def __init__(self):
        self.signed_payloads: list[str] = []

    def sign(self, *, payload_json, **kw):
        self.signed_payloads.append(payload_json)
        return f'SIGNED::{payload_json[:40]}'


class _OkTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(
            state='ACK', transport_request_id='tr-g2',
            submission_status='DPS_ACK', server_fiscal_no='SFN-G2',
            response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC),
        )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _seed_shift_with_sell(conn, shift_id: str, cash_amount: int) -> None:
    """Seed a shift with one SELL document (cash payment) as ACK.

    The write-path's _get_shift_cash_balance() sums ACK'd docs for the shift,
    so Z_REPORT will compute the final balance from these seeded documents.
    """
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = ?", (shift_id,)).fetchone()
    if existing:
        return

    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-16T08:00:00Z',
    )

    req_id = f'req-{shift_id}-sell'
    pj = json.dumps({'receipt': {'payments': [{'amount': cash_amount, 'type': 'CASH'}]}})
    conn.execute("""
        INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type, fiscal_number,
             backend_profile_id, transport_profile_id, channel_owner,
             payload_json, payload_sha256, status, response_deadline_at)
        VALUES (?, ?, 'CHECKBOX_REST', 'SELL', 'FN-DEV-0001',
                'backend_checkbox_default', 'transport_dps_grpc_default', 'test',
                ?, 'sha', 'DONE', '2026-04-16T23:00:00Z')
    """, (req_id, f'idem-{shift_id}-sell', pj))
    conn.execute("""
        INSERT INTO fiscal_documents
            (document_id, request_id, fiscal_number, doc_type, state, fs_mode,
             lnd, backend_profile_id, transport_profile_id,
             submission_status, payload_json, payload_sha256, business_ts, shift_id)
        VALUES (?, ?, 'FN-DEV-0001', 'SELL', 'ACK', 'ONLINE',
                400, 'backend_checkbox_default', 'transport_dps_grpc_default',
                'DPS_ACK', ?, 'sha', '2026-04-16T12:00:00Z', ?)
    """, (f'doc-{shift_id}-sell', req_id, pj, shift_id))
    conn.execute("UPDATE node_state SET next_lnd = 401 WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()


def _enqueue_z_report(conn, rid: str) -> None:
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 16, 22, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'Z_REPORT', 'goods': [], 'payments': [], 'totals': {}}},
        payload_sha256=f'sha-{rid}',
        trace_context=TraceContext(
            source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{rid}',
        ),
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
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-16T23:00:00Z',
    )
    conn.commit()


def _close_shift(conn, shift_id: str) -> None:
    """Manually close shift so SHIFT_OPEN guard allows opening a new one."""
    conn.execute("UPDATE shifts SET state = 'CLOSED' WHERE shift_id = ?", (shift_id,))
    conn.commit()


def _enqueue_shift_open(conn, rid: str) -> None:
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SHIFT_OPEN,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 17, 8, 0, 0, tzinfo=UTC),
        payload={},
        payload_sha256=f'sha-{rid}',
        trace_context=TraceContext(
            source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{rid}',
        ),
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
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-17T08:01:00Z',
    )
    conn.commit()


# ---------------------------------------------------------------------------
# CVO1 — preserve mode: Z_REPORT writes balance → SHIFT_OPEN SM=balance
# ---------------------------------------------------------------------------

def test_cvo1_preserve_mode_carryover(conn) -> None:
    """Full cross-shift carry-over (preserve mode):
    Shift 1 has SELL(cash=100000) → Z_REPORT ACK writes last_cash_balance=100000
    → Shift 2 SHIFT_OPEN DPS XML has SM="100000".
    """
    conn.execute(
        "UPDATE node_state SET cash_balance_mode = 'preserve' WHERE fiscal_number = 'FN-DEV-0001'"
    )
    conn.commit()

    _seed_shift_with_sell(conn, shift_id='shift-cvo1', cash_amount=100000)

    spy = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=spy, transport_client=_OkTransport(), tax_number='TN')

    # Step 1: Z_REPORT → ACK → last_cash_balance written
    _enqueue_z_report(conn, 'req-cvo1-z')
    z_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert z_result.outcome == 'ACK', f'Z_REPORT must ACK: {z_result.canonical_error}'

    # Verify last_cash_balance was written
    node = NodeStateRepository.get_state(conn, 'FN-DEV-0001')
    assert node is not None
    assert node.last_cash_balance == 100000, (
        f'last_cash_balance must be 100000 after Z_REPORT, got {node.last_cash_balance}'
    )

    # Step 2: Close shift 1 so SHIFT_OPEN guard allows a new shift
    _close_shift(conn, 'shift-cvo1')

    # Step 3: SHIFT_OPEN for shift 2 → DPS XML must have SM="100000"
    spy.signed_payloads.clear()
    _enqueue_shift_open(conn, 'req-cvo1-open2')
    open_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert open_result.outcome == 'ACK', f'SHIFT_OPEN must ACK: {open_result.canonical_error}'

    assert len(spy.signed_payloads) >= 1, 'Expected at least one sign() call for SHIFT_OPEN'
    shift_open_xml = spy.signed_payloads[-1]
    assert 'SM="100000"' in shift_open_xml, (
        f'SHIFT_OPEN DPS XML must carry SM="100000" (carry-over). Got: {shift_open_xml}'
    )


# ---------------------------------------------------------------------------
# CVO2 — reset mode: Z_REPORT writes 0 → SHIFT_OPEN SM="0"
# ---------------------------------------------------------------------------

def test_cvo2_reset_mode_no_carryover(conn) -> None:
    """Full cross-shift, reset mode:
    Shift 1 has SELL(cash=100000) → Z_REPORT ACK writes last_cash_balance=0 (reset)
    → Shift 2 SHIFT_OPEN DPS XML has SM="0".
    """
    conn.execute(
        "UPDATE node_state SET cash_balance_mode = 'reset' WHERE fiscal_number = 'FN-DEV-0001'"
    )
    conn.commit()

    _seed_shift_with_sell(conn, shift_id='shift-cvo2', cash_amount=100000)

    spy = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=spy, transport_client=_OkTransport(), tax_number='TN')

    # Z_REPORT → ACK → last_cash_balance must be 0 (reset mode writes 0)
    _enqueue_z_report(conn, 'req-cvo2-z')
    z_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert z_result.outcome == 'ACK', f'Z_REPORT must ACK: {z_result.canonical_error}'

    node = NodeStateRepository.get_state(conn, 'FN-DEV-0001')
    assert node is not None
    assert node.last_cash_balance == 0, (
        f'last_cash_balance must be 0 in reset mode, got {node.last_cash_balance}'
    )

    # Close shift 1, open shift 2
    _close_shift(conn, 'shift-cvo2')
    spy.signed_payloads.clear()
    _enqueue_shift_open(conn, 'req-cvo2-open2')
    open_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert open_result.outcome == 'ACK', f'SHIFT_OPEN must ACK: {open_result.canonical_error}'

    shift_open_xml = spy.signed_payloads[-1]
    assert 'SM="0"' in shift_open_xml, (
        f'SHIFT_OPEN DPS XML must have SM="0" in reset mode. Got: {shift_open_xml}'
    )


# ---------------------------------------------------------------------------
# CVO3 — empty shift (no fiscal docs): Z_REPORT writes 0 → SHIFT_OPEN SM="0"
# ---------------------------------------------------------------------------

def test_cvo3_empty_shift_zero_balance(conn) -> None:
    """Shift with no SELL docs: Z_REPORT writes last_cash_balance=0,
    even in preserve mode. SHIFT_OPEN XML has SM="0".
    """
    conn.execute(
        "UPDATE node_state SET cash_balance_mode = 'preserve', last_cash_balance = 0 "
        "WHERE fiscal_number = 'FN-DEV-0001'"
    )
    ShiftRepository.create_shift(
        conn, shift_id='shift-cvo3', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-16T08:00:00Z',
    )
    conn.execute("UPDATE node_state SET next_lnd = 500 WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()

    spy = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=spy, transport_client=_OkTransport(), tax_number='TN')

    _enqueue_z_report(conn, 'req-cvo3-z')
    z_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert z_result.outcome == 'ACK', f'Z_REPORT must ACK: {z_result.canonical_error}'

    node = NodeStateRepository.get_state(conn, 'FN-DEV-0001')
    assert node is not None
    assert node.last_cash_balance == 0, (
        f'Empty shift: last_cash_balance must be 0, got {node.last_cash_balance}'
    )

    _close_shift(conn, 'shift-cvo3')
    spy.signed_payloads.clear()
    _enqueue_shift_open(conn, 'req-cvo3-open2')
    open_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert open_result.outcome == 'ACK', f'SHIFT_OPEN must ACK: {open_result.canonical_error}'

    shift_open_xml = spy.signed_payloads[-1]
    assert 'SM="0"' in shift_open_xml, (
        f'Empty shift: SHIFT_OPEN SM must be "0". Got: {shift_open_xml}'
    )
