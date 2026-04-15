"""
Sprint 3 / step 4 — excise return/void lifecycle tests.

Coverage matrix:
  ER1 — SELL marks item SOLD, RETURN transitions it to RETURNED
  ER2 — after RETURN, same mark can be re-sold (partial index allows re-use)
  ER3 — RETURN with mark not in SOLD status is a no-op (no error, no transition)
  ER4 — RETURN with malformed excise mark rejected by validator
  ER5 — reconciliation ACK triggers excise side effects (SELL→SOLD, RETURN→RETURNED)
  ER6 — async RETURN (SENT → reconciliation ACK) transitions mark to RETURNED
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import CanonicalErrorCode, DocumentState, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import ExciseRepository, InboxRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# Stubs
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

class _StubCrypto:
    def sign(self, *, payload_json, **kw):
        return f'signed::{payload_json}'


class _StubTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(state='ACK', transport_request_id='tr-er',
                          submission_status='DONE', server_fiscal_no='SFN-ER',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


def _cmd(req_id, op, payload):
    return CanonicalFiscalCommand(
        request_id=req_id, idempotency_key=f'idem-{req_id}',
        protocol=Protocol.CHECKBOX_REST, operation_type=op,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='test', external_request_id=f'ext-{req_id}',
        business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
        payload=payload, payload_sha256=f'sha-{req_id}',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1234,
                                  session_id='s1', correlation_id=f'c-{req_id}'),
        correlation_id=f'c-{req_id}',
    )


def _enqueue(conn, cmd):
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-12T12:01:00Z',
    )
    conn.commit()


SELL_PAYLOAD = {
    'receipt': {
        'type': 'SELL',
        'goods': [{'name': 'Wine', 'uktzed': '2204', 'excise_barcodes': ['XRET000001'],
                    'price': 5000, 'quantity': 1000, 'sum': 5000}],
        'payments': [{'amount': 5000, 'type': 'CASH'}],
        'totals': {'total_sum': 5000},
    },
}

RETURN_PAYLOAD = {
    'receipt': {
        'type': 'RETURN',
        'goods': [{'name': 'Wine', 'uktzed': '2204', 'excise_barcodes': ['XRET000001'],
                    'price': 5000, 'quantity': 1000, 'sum': 5000}],
        'payments': [{'amount': 5000, 'type': 'CASH'}],
        'totals': {'total_sum': 5000},
        'related_receipt_id': 'rcpt-sell-001',
    },
}


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# ER1 — SELL → SOLD, RETURN → RETURNED
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_er1_sell_then_return_transitions_mark(conn) -> None:
    """SELL marks excise as SOLD; RETURN of same mark transitions to RETURNED."""
    ShiftRepository.create_shift(
        conn, shift_id='shift-er1', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    # SELL
    _enqueue(conn, _cmd('req-er1-sell', OperationType.SELL, SELL_PAYLOAD))
    worker = WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_StubTransport())
    sell_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert sell_result.outcome == 'ACK'

    # Verify SOLD
    row = conn.execute(
        "SELECT status FROM excise_marks WHERE normalized_mark_code = 'XRET000001'"
    ).fetchone()
    assert row is not None and row[0] == 'SOLD', f'Mark must be SOLD after SELL, got {row}'

    # RETURN
    _enqueue(conn, _cmd('req-er1-ret', OperationType.RETURN, RETURN_PAYLOAD))
    ret_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert ret_result.outcome == 'ACK'

    # Verify RETURNED
    row = conn.execute(
        "SELECT status, returned_at FROM excise_marks WHERE normalized_mark_code = 'XRET000001'"
    ).fetchone()
    assert row[0] == 'RETURNED', f'Mark must be RETURNED after return, got {row[0]}'
    assert row[1] is not None, 'returned_at must be set'


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# ER2 — after RETURN, same mark can be re-sold
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_er2_returned_mark_can_be_resold(conn) -> None:
    """After RETURNED, the partial index allows a new SELL of the same mark."""
    ShiftRepository.create_shift(
        conn, shift_id='shift-er2', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    worker = WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_StubTransport())

    # SELL → SOLD
    _enqueue(conn, _cmd('req-er2-sell1', OperationType.SELL, SELL_PAYLOAD))
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # RETURN → RETURNED
    _enqueue(conn, _cmd('req-er2-ret', OperationType.RETURN, RETURN_PAYLOAD))
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # Re-SELL same mark → must succeed (not blocked by duplicate protection)
    _enqueue(conn, _cmd('req-er2-sell2', OperationType.SELL, SELL_PAYLOAD))
    resell = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert resell.outcome == 'ACK', f'Re-sell after return must succeed: {resell.outcome}'

    # Two mark rows now: one RETURNED, one SOLD
    rows = conn.execute(
        "SELECT status FROM excise_marks WHERE normalized_mark_code = 'XRET000001' ORDER BY created_at"
    ).fetchall()
    statuses = [r[0] for r in rows]
    assert 'RETURNED' in statuses and 'SOLD' in statuses


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# ER3 — RETURN of unsold mark is no-op
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_er3_return_unsold_mark_is_noop(conn) -> None:
    """RETURN with excise mark that was never sold: no error, mark not found in DB."""
    ShiftRepository.create_shift(
        conn, shift_id='shift-er3', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    _enqueue(conn, _cmd('req-er3-ret', OperationType.RETURN, RETURN_PAYLOAD))
    worker = WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_StubTransport())
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Return of unsold mark must not error: {result.outcome}'


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# ER4 — RETURN with malformed excise mark rejected by validator
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_er4_return_malformed_mark_rejected(conn) -> None:
    """RETURN with malformed excise barcode must be rejected before transport."""
    ShiftRepository.create_shift(
        conn, shift_id='shift-er4', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    bad_payload = {
        'receipt': {
            'type': 'RETURN',
            'goods': [{'name': 'Wine', 'uktzed': '2204', 'excise_barcodes': ['bad-mark'],
                        'price': 5000, 'quantity': 1000, 'sum': 5000}],
            'payments': [{'amount': 5000, 'type': 'CASH'}],
            'totals': {'total_sum': 5000},
            'related_receipt_id': 'rcpt-001',
        },
    }

    _enqueue(conn, _cmd('req-er4', OperationType.RETURN, bad_payload))

    class _NoTransport:
        def send(self, **kw): raise AssertionError('Transport must not be called')
    class _NoCrypto:
        def sign(self, **kw): raise AssertionError('Crypto must not be called')

    worker = WritePathWorker(crypto_provider=_NoCrypto(), transport_client=_NoTransport())
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ERROR'
    assert result.canonical_error.code == CanonicalErrorCode.INVALID_RECEIPT_DATA


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# ER5 — reconciliation ACK triggers excise side effects
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_er5_reconciliation_excise_side_effects(conn) -> None:
    """Reconciliation ACK must transition SELL marks to SOLD and RETURN marks to RETURNED.

    Tests _apply_excise_side_effects_locked directly — the reconciliation method
    that fires during poll-based ACK for async documents.
    """
    import json
    from prro_gateway.services.reconciliation import ReconciliationService

    # Seed a fiscal document + inbox row (FK targets)
    conn.execute(
        """INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256, status)
        VALUES ('req-er5', 'idem-er5', 'CHECKBOX_REST', 'SELL', 'FN-DEV-0001', '{}', 'sha-er5', 'DONE')"""
    )
    from prro_gateway.repositories import FiscalDocumentRepository
    FiscalDocumentRepository.create_prepared(
        conn, document_id='doc-sell-er5', request_id='req-er5',
        fiscal_number='FN-DEV-0001', lnd=99900, doc_type='SELL',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        fs_mode='ONLINE', business_ts='2026-04-12T12:00:00Z',
        payload_json='{}', payload_sha256='sha-er5',
    )
    # Seed excise mark as RESERVED (simulating write-path reserve_marks)
    conn.execute(
        """INSERT INTO excise_marks
            (excise_mark_id, normalized_mark_code, raw_mark_code, status,
             receipt_document_id, receipt_item_no, product_code)
        VALUES ('excise-er5-1', 'XREC000001', 'XREC000001', 'RESERVED', 'doc-sell-er5', 1, 'ALC001')"""
    )
    conn.commit()

    # Simulate reconciliation ACK for the SELL document
    # payload_json stores full CanonicalFiscalCommand shape: receipt is under 'payload'
    class _SellDoc:
        document_id = 'doc-sell-er5'
        doc_type = 'SELL'
        payload_json = json.dumps({
            'payload': {'receipt': {'type': 'SELL', 'goods': [{'excise_barcodes': ['XREC000001']}]}}
        })

    ReconciliationService._apply_excise_side_effects_locked(conn, doc=_SellDoc(), ack_at='2026-04-12T12:00:00Z')

    row = conn.execute("SELECT status, sold_at FROM excise_marks WHERE normalized_mark_code = 'XREC000001'").fetchone()
    assert row[0] == 'SOLD', f'Reconciliation ACK must mark SOLD, got {row[0]}'
    assert row[1] is not None

    # Now simulate reconciliation ACK for a RETURN of the same mark
    class _ReturnDoc:
        document_id = 'doc-ret-er5'
        doc_type = 'RETURN'
        payload_json = json.dumps({
            'payload': {'receipt': {'type': 'RETURN', 'goods': [{'excise_barcodes': ['XREC000001']}]}}
        })

    ReconciliationService._apply_excise_side_effects_locked(conn, doc=_ReturnDoc(), ack_at='2026-04-12T12:05:00Z')

    row = conn.execute("SELECT status, returned_at FROM excise_marks WHERE normalized_mark_code = 'XREC000001'").fetchone()
    assert row[0] == 'RETURNED', f'Reconciliation RETURN ACK must mark RETURNED, got {row[0]}'
    assert row[1] is not None


# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000
# ER6 — async RETURN → SENT → reconciliation ACK → RETURNED
# BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000BAAD000000

def test_er6_async_return_reconciliation_transitions_mark(conn) -> None:
    """Full async RETURN: SELL→ACK (mark SOLD), RETURN→SENT (mark still SOLD),
    then reconciliation ACK for RETURN → mark RETURNED.

    Uses write-path for SELL (immediate ACK) and RETURN (async SENT),
    then calls reconciliation excise handler directly for the final ACK.
    """
    import json as _json

    ShiftRepository.create_shift(
        conn, shift_id='shift-er6', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    # SELL → immediate ACK → mark SOLD
    _enqueue(conn, _cmd('req-er6-sell', OperationType.SELL, SELL_PAYLOAD))
    worker = WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_StubTransport())
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    row = conn.execute("SELECT status FROM excise_marks WHERE normalized_mark_code = 'XRET000001'").fetchone()
    assert row[0] == 'SOLD'

    # RETURN → async SENT (transport returns SENT, not ACK)
    class _AsyncTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='SENT', transport_request_id='tr-er6-ret',
                              submission_status='PROCESSING', sent_at=datetime.now(UTC))

    _enqueue(conn, _cmd('req-er6-ret', OperationType.RETURN, RETURN_PAYLOAD))
    async_worker = WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_AsyncTransport())
    ret_result = async_worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert ret_result.outcome == 'SENT'

    # Mark still SOLD — RETURN hasn't been confirmed yet
    row = conn.execute("SELECT status FROM excise_marks WHERE normalized_mark_code = 'XRET000001'").fetchone()
    assert row[0] == 'SOLD', f'Mark must still be SOLD after SENT, got {row[0]}'

    # Reconciliation ACK — simulate poll returning ACK for the RETURN document
    ret_doc = conn.execute(
        "SELECT document_id, doc_type, payload_json FROM fiscal_documents WHERE request_id = 'req-er6-ret'"
    ).fetchone()
    assert ret_doc is not None

    class _ReconDoc:
        document_id = ret_doc[0]
        doc_type = ret_doc[1]
        payload_json = ret_doc[2]

    from prro_gateway.services.reconciliation import ReconciliationService
    ReconciliationService._apply_excise_side_effects_locked(
        conn, doc=_ReconDoc(), ack_at='2026-04-12T12:10:00Z',
    )

    # Now mark must be RETURNED
    row = conn.execute("SELECT status, returned_at FROM excise_marks WHERE normalized_mark_code = 'XRET000001'").fetchone()
    assert row[0] == 'RETURNED', f'After reconciliation ACK, mark must be RETURNED, got {row[0]}'
    assert row[1] is not None
