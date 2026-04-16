"""Sprint 10 step 3: Cash balance calculation + carry-over.

Tests:
  CB1: balance = SERVICE_IN - SERVICE_OUT + SELL(cash) - RETURN(cash) - CASH_WITHDRAWAL
  CB4: SHIFT_OPEN SM = last_cash_balance when preserve (in signed XML)
  CB5: SHIFT_OPEN SM = 0 when reset (in signed XML)
  CB6: Z_REPORT writes last_cash_balance to node_state
  CB7: negative balance clamped to 0
  CB2: SERVICE_OUT > balance → reject before sign
  CB3: CASH_WITHDRAWAL > balance → reject before sign
  CB2a: SERVICE_OUT == balance → passes
  CB3a: CASH_WITHDRAWAL == balance → passes
"""
from __future__ import annotations

import json
from datetime import datetime, UTC

from prro_gateway.enums import CanonicalErrorCode, DocumentState, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, NodeStateRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


def _seed_shift_with_docs(conn, shift_id='shift-cb'):
    """Create shift + acknowledged docs for cash balance testing."""
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = ?", (shift_id,)).fetchone()
    if existing:
        return
    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()

    # Seed acknowledged fiscal documents
    docs = [
        ('doc-cb-svc-in', 'SERVICE_IN', json.dumps({'service_sum': 100000})),
        ('doc-cb-sell', 'SELL', json.dumps({
            'receipt': {'payments': [{'amount': 50000, 'type': 'CASH'}]},
        })),
        ('doc-cb-ret', 'RETURN', json.dumps({
            'receipt': {'payments': [{'amount': 10000, 'type': 'CASH'}]},
        })),
        ('doc-cb-svc-out', 'SERVICE_OUT', json.dumps({'service_sum': 20000})),
        ('doc-cb-cw', 'CASH_WITHDRAWAL', json.dumps({'cash_withdrawal_sum': 5000})),
    ]
    # Advance next_lnd past seeded docs
    conn.execute("UPDATE node_state SET next_lnd = 100 WHERE fiscal_number = 'FN-DEV-0001'")
    for i, (doc_id, doc_type, pj) in enumerate(docs, start=90):
        req_id = f'req-{doc_id}'
        conn.execute("""
            INSERT OR IGNORE INTO ingress_inbox (
                request_id, idempotency_key, protocol, operation_type, fiscal_number,
                backend_profile_id, transport_profile_id, channel_owner,
                payload_json, payload_sha256, status, response_deadline_at
            ) VALUES (?, ?, 'CHECKBOX_REST', ?, 'FN-DEV-0001', 'backend_checkbox_default',
                'transport_dps_grpc_default', 'test', ?, 'sha', 'DONE', '2026-04-14T23:00:00Z')
        """, (req_id, f'idem-{doc_id}', doc_type, pj))
        conn.execute("""
            INSERT INTO fiscal_documents (
                document_id, request_id, fiscal_number, doc_type, state, fs_mode,
                lnd, backend_profile_id, transport_profile_id,
                submission_status, payload_json, payload_sha256, business_ts,
                shift_id
            ) VALUES (?, ?, 'FN-DEV-0001', ?, 'ACK', 'ONLINE',
                ?, 'backend_checkbox_default', 'transport_dps_grpc_default',
                'DPS_ACK', ?, 'sha', '2026-04-14T12:00:00Z', ?)
        """, (doc_id, req_id, doc_type, i, pj, shift_id))
    conn.commit()


# ---------------------------------------------------------------------------
# CB1 — balance formula
# ---------------------------------------------------------------------------

def test_cb1_balance_formula(conn) -> None:
    """balance = SERVICE_IN(100000) + SELL_CASH(50000) - RETURN_CASH(10000)
                - SERVICE_OUT(20000) - CASH_WITHDRAWAL(5000) = 115000"""
    _seed_shift_with_docs(conn)
    balance = WritePathWorker._get_shift_cash_balance(conn, 'shift-cb')
    assert balance == 115000, f'Expected 115000, got {balance}'


# ---------------------------------------------------------------------------
# CB4 — SHIFT_OPEN SM = last_cash_balance when preserve
# ---------------------------------------------------------------------------

def test_cb4_shift_open_preserve(conn) -> None:
    """When cash_balance_mode='preserve', SHIFT_OPEN XML SM = last_cash_balance."""
    conn.execute(
        "UPDATE node_state SET last_cash_balance = 75000, cash_balance_mode = 'preserve' WHERE fiscal_number = 'FN-DEV-0001'",
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-cb4', idempotency_key='idem-cb4',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SHIFT_OPEN,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-cb4',
        business_ts=datetime(2026, 4, 14, 8, 0, 0, tzinfo=UTC),
        payload={},
        payload_sha256='sha-cb4',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-cb4'),
        correlation_id='c-cb4',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T08:01:00Z',
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
            return SendResult(state='ACK', transport_request_id='tr-cb4',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    assert len(signed_xmls) == 1
    xml = signed_xmls[0]
    assert 'SM="75000"' in xml, f'SHIFT_OPEN SM must be 75000 (carry-over). Got: {xml}'


# ---------------------------------------------------------------------------
# CB5 — SHIFT_OPEN SM = 0 when reset
# ---------------------------------------------------------------------------

def test_cb5_shift_open_reset(conn) -> None:
    """When cash_balance_mode='reset', SHIFT_OPEN XML SM = 0 even if last_cash_balance > 0."""
    conn.execute(
        "UPDATE node_state SET last_cash_balance = 75000, cash_balance_mode = 'reset' WHERE fiscal_number = 'FN-DEV-0001'",
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-cb5', idempotency_key='idem-cb5',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SHIFT_OPEN,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-cb5',
        business_ts=datetime(2026, 4, 14, 8, 0, 0, tzinfo=UTC),
        payload={},
        payload_sha256='sha-cb5',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-cb5'),
        correlation_id='c-cb5',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T08:01:00Z',
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
            return SendResult(state='ACK', transport_request_id='tr-cb5',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'
    xml = signed_xmls[0]
    assert 'SM="0"' in xml, f'SHIFT_OPEN SM must be 0 (reset mode). Got: {xml}'


# ---------------------------------------------------------------------------
# CB6 — Z_REPORT writes last_cash_balance
# ---------------------------------------------------------------------------

def test_cb6_zreport_writes_last_cash_balance(conn) -> None:
    """Z_REPORT must write final shift cash balance to node_state.last_cash_balance."""
    _seed_shift_with_docs(conn, shift_id='shift-cb6')
    # Expected balance from seeded docs: 115000

    # Enqueue Z_REPORT
    cmd = CanonicalFiscalCommand(
        request_id='req-cb6-z', idempotency_key='idem-cb6-z',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-cb6-z',
        business_ts=datetime(2026, 4, 14, 22, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'Z_REPORT', 'goods': [], 'payments': [], 'totals': {}}},
        payload_sha256='sha-cb6-z',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-cb6-z'),
        correlation_id='c-cb6-z',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T23:00:00Z',
    )
    conn.commit()

    class _Crypto:
        def sign(self, *, payload_json, **kw): return 'signed'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-cb6',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    conn.execute("UPDATE node_state SET cash_balance_mode = 'preserve' WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Got: {result.canonical_error}'

    # Verify last_cash_balance written
    row = conn.execute(
        "SELECT last_cash_balance FROM node_state WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    assert row is not None
    assert row[0] == 115000, f'last_cash_balance must be 115000 after Z_REPORT. Got: {row[0]}'


# ---------------------------------------------------------------------------
# CB7 — negative balance clamped to 0, Z_REPORT does not crash
# ---------------------------------------------------------------------------

def test_cb7_negative_balance_clamped(conn) -> None:
    """Shift with only SERVICE_OUT (no prior SERVICE_IN) → negative raw balance.
    Z_REPORT must not crash — last_cash_balance clamped to 0."""
    shift_id = 'shift-cb7'
    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    conn.commit()

    # Seed only SERVICE_OUT → negative balance
    req_id = 'req-cb7-svc-out'
    pj = json.dumps({'service_sum': 50000})
    conn.execute("""
        INSERT INTO ingress_inbox (request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            payload_json, payload_sha256, status, response_deadline_at)
        VALUES (?, ?, 'CHECKBOX_REST', 'SERVICE_OUT', 'FN-DEV-0001', 'backend_checkbox_default',
            'transport_dps_grpc_default', 'test', ?, 'sha', 'DONE', '2026-04-14T23:00:00Z')
    """, (req_id, 'idem-cb7-svc-out', pj))
    conn.execute("""
        INSERT INTO fiscal_documents (document_id, request_id, fiscal_number, doc_type, state, fs_mode,
            lnd, backend_profile_id, transport_profile_id,
            submission_status, payload_json, payload_sha256, business_ts, shift_id)
        VALUES ('doc-cb7-svc-out', ?, 'FN-DEV-0001', 'SERVICE_OUT', 'ACK', 'ONLINE',
            200, 'backend_checkbox_default', 'transport_dps_grpc_default',
            'DPS_ACK', ?, 'sha', '2026-04-14T12:00:00Z', ?)
    """, (req_id, pj, shift_id))
    conn.execute("UPDATE node_state SET next_lnd = 201, cash_balance_mode = 'preserve' WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()

    # Verify raw balance is negative
    raw = WritePathWorker._get_shift_cash_balance(conn, shift_id)
    assert raw == -50000, f'Raw balance should be -50000, got {raw}'

    # Enqueue Z_REPORT
    cmd = CanonicalFiscalCommand(
        request_id='req-cb7-z', idempotency_key='idem-cb7-z',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-cb7-z',
        business_ts=datetime(2026, 4, 14, 22, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'Z_REPORT', 'goods': [], 'payments': [], 'totals': {}}},
        payload_sha256='sha-cb7-z',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-cb7-z'),
        correlation_id='c-cb7-z',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T23:00:00Z',
    )
    conn.commit()

    class _Crypto:
        def sign(self, *, payload_json, **kw): return 'signed'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-cb7',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Z_REPORT must not crash on negative balance. Got: {result.canonical_error}'

    row = conn.execute(
        "SELECT last_cash_balance FROM node_state WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    assert row[0] == 0, f'last_cash_balance must be clamped to 0. Got: {row[0]}'


# ---------------------------------------------------------------------------
# Helper: seed shift with known cash balance
# ---------------------------------------------------------------------------

def _seed_shift_with_balance(conn, shift_id, balance_amount):
    """Create shift + one SERVICE_IN to establish known cash balance."""
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = ?", (shift_id,)).fetchone()
    if existing:
        return
    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-14T08:00:00Z',
    )
    req_id = f'req-{shift_id}-svc-in'
    pj = json.dumps({'service_sum': balance_amount})
    conn.execute("""
        INSERT INTO ingress_inbox (request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            payload_json, payload_sha256, status, response_deadline_at)
        VALUES (?, ?, 'CHECKBOX_REST', 'SERVICE_IN', 'FN-DEV-0001', 'backend_checkbox_default',
            'transport_dps_grpc_default', 'test', ?, 'sha', 'DONE', '2026-04-14T23:00:00Z')
    """, (req_id, f'idem-{shift_id}-svc-in', pj))
    conn.execute("""
        INSERT INTO fiscal_documents (document_id, request_id, fiscal_number, doc_type, state, fs_mode,
            lnd, backend_profile_id, transport_profile_id,
            submission_status, payload_json, payload_sha256, business_ts, shift_id)
        VALUES (?, ?, 'FN-DEV-0001', 'SERVICE_IN', 'ACK', 'ONLINE',
            300, 'backend_checkbox_default', 'transport_dps_grpc_default',
            'DPS_ACK', ?, 'sha', '2026-04-14T08:00:00Z', ?)
    """, (f'doc-{shift_id}-svc-in', req_id, pj, shift_id))
    conn.execute("UPDATE node_state SET next_lnd = 301 WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()


def _enqueue_op(conn, rid, op_type, payload):
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.CHECKBOX_REST, operation_type=op_type,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload=payload, payload_sha256=f'sha-{rid}',
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
        self.call_count = 0
    def sign(self, **kw):
        self.call_count += 1
        return 'signed'
    def sign_raw(self, *, data, document_id=None):
        self.call_count += 1
        return b'\x30\x82SIGNED'


class _OkTransport2:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(state='ACK', transport_request_id='tr',
                          submission_status='DPS_ACK', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


# ---------------------------------------------------------------------------
# CB2 — SERVICE_OUT > balance → reject before sign
# ---------------------------------------------------------------------------

def test_cb2_service_out_exceeds_balance_rejected(conn) -> None:
    _seed_shift_with_balance(conn, 'shift-cb2', 50000)
    _enqueue_op(conn, 'req-cb2', OperationType.SERVICE_OUT, {
        'service_sum': 60000,
        'receipt': {'type': 'SERVICE_OUT', 'goods': [], 'payments': [], 'totals': {}},
    })
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport2(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert '60000' in result.canonical_error.message
    assert '50000' in result.canonical_error.message
    assert crypto.call_count == 0, 'Crypto must NOT be called'


# ---------------------------------------------------------------------------
# CB3 — CASH_WITHDRAWAL > balance → reject before sign
# ---------------------------------------------------------------------------

def test_cb3_cash_withdrawal_exceeds_balance_rejected(conn) -> None:
    _seed_shift_with_balance(conn, 'shift-cb3', 50000)
    _enqueue_op(conn, 'req-cb3', OperationType.CASH_WITHDRAWAL, {
        'cash_withdrawal_sum': 60000,
        'receipt': {'type': 'CASH_WITHDRAWAL', 'goods': [], 'payments': [], 'totals': {}},
    })
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_OkTransport2(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA
    assert '60000' in result.canonical_error.message
    assert '50000' in result.canonical_error.message
    assert crypto.call_count == 0, 'Crypto must NOT be called'


# ---------------------------------------------------------------------------
# CB2a — SERVICE_OUT == balance → passes
# ---------------------------------------------------------------------------

def test_cb2a_service_out_equals_balance_passes(conn) -> None:
    _seed_shift_with_balance(conn, 'shift-cb2a', 50000)
    _enqueue_op(conn, 'req-cb2a', OperationType.SERVICE_OUT, {
        'service_sum': 50000,
        'receipt': {'type': 'SERVICE_OUT', 'goods': [], 'payments': [], 'totals': {}},
    })
    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport2(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'SERVICE_OUT == balance should pass. Got: {result.canonical_error}'


# ---------------------------------------------------------------------------
# CB3a — CASH_WITHDRAWAL == balance → passes
# ---------------------------------------------------------------------------

def test_cb3a_cash_withdrawal_equals_balance_passes(conn) -> None:
    _seed_shift_with_balance(conn, 'shift-cb3a', 50000)
    _enqueue_op(conn, 'req-cb3a', OperationType.CASH_WITHDRAWAL, {
        'cash_withdrawal_sum': 50000,
        'receipt': {'type': 'CASH_WITHDRAWAL', 'goods': [{'name': 'Гривня', 'price': 50000, 'quantity': 1000, 'sum': 50000}],
                    'payments': [{'amount': 50000, 'type': 'CASH'}], 'totals': {'total_sum': 50000}},
    })
    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport2(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'CASH_WITHDRAWAL == balance should pass. Got: {result.canonical_error}'
