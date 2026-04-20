from __future__ import annotations

import logging
from datetime import UTC, datetime
from types import SimpleNamespace

from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState, TransportKind
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.utils.json_codec import dumps_json
from prro_gateway.ports import PollResult, SendResult
from prro_gateway.repositories import FiscalDocumentRepository, InboxRepository, ShiftRepository
from prro_gateway.services.reconciliation import ReconciliationService
from prro_gateway.services.write_path import WritePathWorker
from prro_gateway.transports import ProfileAwareTransportRouter


class FakeCrypto:
    def sign(self, *, document_id: str, payload_json: str) -> str:
        return f"<signed id='{document_id}'/>"


class FakeHandler:
    def __init__(self, *, send_result: SendResult | None = None, poll_result: PollResult | None = None):
        self._send_result = send_result or SendResult(
            transport_request_id='req-1',
            submission_status='SENT',
            sent_at=datetime.now(UTC),
            response_json='{"accepted": true}',
        )
        self._poll_result = poll_result or PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC), response_json='{"status": "ACK"}')
        self.send_calls = 0
        self.poll_calls = 0

    def send(self, **kwargs):
        self.send_calls += 1
        return self._send_result

    def poll_status(self, **kwargs):
        self.poll_calls += 1
        return self._poll_result


def _seed_shift(conn, fiscal_number='FN-DEV-0001'):
    ShiftRepository.create_shift(
        conn,
        shift_id='shift-1',
        fiscal_number=fiscal_number,
        state=ShiftState.OPENED,
        open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST,
        integration_owner='pos-1',
        channel_lock_acquired_at=datetime.now(UTC).isoformat(),
    )
    conn.commit()


def _accept_sell(conn, fiscal_number='FN-DEV-0001'):
    payload = {
        'currency': 'UAH',
        'cashier_id': 'cashier-1',
        'goods_count': 1,
        'payments_count': 1,
        'receipt': {
            'type': 'SELL',
            'goods': [{'item_id': 'item-1', 'item_no': 1, 'code': 'SKU-1', 'name': 'Water', 'price': 5000, 'quantity': 1000, 'sum': 5000, 'excise_barcodes': []}],
            'payments': [{'payment_id': 'pay-1', 'payment_type': 'CASH', 'amount': 5000}],
            'totals': {'total_sum': 5000},
            'delivery': None,
            'related_receipt_id': None,
            'previous_receipt_id': None,
            'technical_return': None,
            'rounding_enabled': False,
        },
    }
    cmd = CanonicalFiscalCommand(
        request_id='req-sell-1',
        idempotency_key='idem-req-sell-1',
        protocol=Protocol.CHECKBOX_REST,
        operation_type=OperationType.SELL,
        fiscal_number=fiscal_number,
        route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='pos-1',
        external_request_id='ext-req-sell-1',
        business_ts=datetime(2026, 3, 28, 12, 0, 0, tzinfo=UTC),
        payload=payload,
        payload_sha256='sha-req-sell-1',
        trace_context=TraceContext(source_ip='10.0.0.10', source_port=12000, session_id='sess-1', correlation_id='corr-req-sell-1'),
        correlation_id='corr-req-sell-1',
    )
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
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
        response_deadline_at='2026-03-28T12:01:00Z',
    )
    conn.commit()
    return cmd


def test_transport_router_dispatch_by_transport_kind(conn):
    handler = FakeHandler()
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: handler})
    result = router.send(
        document_id='doc-1',
        signed_payload='<signed/>',
        fiscal_number='FN-DEV-0001',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
    )
    assert handler.send_calls == 1
    assert result.transport_request_id == 'req-1'


def test_reconciliation_acks_sent_document(conn):
    _seed_shift(conn)
    _accept_sell(conn)
    handler = FakeHandler(
        send_result=SendResult(transport_request_id='req-1', submission_status='SENT', sent_at=datetime.now(UTC), response_json='{"accepted": true}'),
        poll_result=PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC), response_json='{"status": "ACK"}'),
    )
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: handler})
    worker = WritePathWorker(crypto_provider=FakeCrypto(), transport_client=router)
    process_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert process_result.outcome == 'ACK'

    doc = FiscalDocumentRepository.get_by_id(conn, process_result.document_id)
    assert doc is not None
    # simulate crash after SENT but before ACK
    conn.execute("UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL WHERE document_id=?", (doc.document_id,))
    conn.commit()

    svc = ReconciliationService(transport_status_client=router)
    result = svc.reconcile_pending(conn)
    assert result.acked == 1
    doc = FiscalDocumentRepository.get_by_id(conn, doc.document_id)
    assert doc.state == DocumentState.ACK


def test_reconciliation_marks_rejected_document(conn):
    _seed_shift(conn)
    _accept_sell(conn)
    send = SendResult(transport_request_id='req-1', submission_status='SENT', sent_at=datetime.now(UTC))
    poll = PollResult(state=DocumentState.REJECTED.value, submission_status='REJECTED', response_json='{"status": "REJECTED"}')
    handler = FakeHandler(send_result=send, poll_result=poll)
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: handler})
    worker = WritePathWorker(crypto_provider=FakeCrypto(), transport_client=router)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    doc = FiscalDocumentRepository.get_by_id(conn, result.document_id)
    conn.execute("UPDATE fiscal_documents SET state='SENT', ack_at=NULL WHERE document_id=?", (doc.document_id,))
    conn.commit()
    svc = ReconciliationService(transport_status_client=router)
    recon = svc.reconcile_pending(conn)
    assert recon.rejected == 1
    doc = FiscalDocumentRepository.get_by_id(conn, doc.document_id)
    assert doc.state == DocumentState.REJECTED


def test_reconciliation_retryable_increments_attempts(conn):
    _seed_shift(conn)
    _accept_sell(conn)
    send = SendResult(transport_request_id='req-1', submission_status='SENT', sent_at=datetime.now(UTC))
    poll = PollResult(state=DocumentState.ERROR_RETRYABLE.value, submission_status='RETRYABLE', response_json='{"status": "RETRYABLE"}', retryable=True)
    handler = FakeHandler(send_result=send, poll_result=poll)
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: handler})
    worker = WritePathWorker(crypto_provider=FakeCrypto(), transport_client=router)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    doc = FiscalDocumentRepository.get_by_id(conn, result.document_id)
    conn.execute("UPDATE fiscal_documents SET state='SENT', ack_at=NULL WHERE document_id=?", (doc.document_id,))
    conn.commit()
    svc = ReconciliationService(transport_status_client=router)
    recon = svc.reconcile_pending(conn)
    assert recon.retryable == 1
    doc = FiscalDocumentRepository.get_by_id(conn, doc.document_id)
    assert doc.state == DocumentState.ERROR_RETRYABLE
    assert doc.recovery_attempts == 1



def test_reconciliation_promotes_shift_open_pending_to_opened(conn):
    try:
        from prro_gateway.ports import SendResult, PollResult
        from prro_gateway.enums import ShiftState

        class PendingShiftHandler:
            def send(self, **kwargs):
                now = datetime.now(UTC)
                return SendResult(
                    state='SENT',
                    transport_request_id='remote-shift-1',
                    submission_status='CREATED',
                    response_json='{}',
                    sent_at=now,
                )

            def poll_status(self, **kwargs):
                return PollResult(
                    state='ACK',
                    submission_status='OPENED',
                    response_json='{}',
                    ack_at=datetime.now(UTC),
                )

        conn.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            conn,
            request_id='req-shift-open-reconcile',
            idempotency_key='idem-shift-open-reconcile',
            protocol=Protocol.CHECKBOX_REST,
            operation_type=OperationType.SHIFT_OPEN,
            fiscal_number='FN-DEV-0001',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            channel_owner='front-a',
            external_request_id='ext-shift-open-reconcile',
            payload_json=dumps_json(CanonicalFiscalCommand(
                request_id='req-shift-open-reconcile',
                idempotency_key='idem-shift-open-reconcile',
                protocol=Protocol.CHECKBOX_REST,
                operation_type=OperationType.SHIFT_OPEN,
                fiscal_number='FN-DEV-0001',
                route_key='main',
                backend_profile_id='backend_checkbox_default',
                transport_profile_id='transport_checkbox_rest_default',
                channel_owner='front-a',
                external_request_id='ext-shift-open-reconcile',
                business_ts=datetime(2026, 1, 1, 10, 0, 0, tzinfo=UTC),
                payload={'receipt': {'type': 'SHIFT_OPEN'}},
                payload_sha256='sha-shift-open-reconcile',
            ).model_dump(mode='json')),
            payload_sha256='sha-shift-open-reconcile',
        )
        conn.execute(
            "UPDATE transport_profiles SET config_json = ? WHERE transport_profile_id = 'transport_checkbox_rest_default'",
            (dumps_json({'require_local_sign': False}),),
        )
        conn.commit()

        router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: PendingShiftHandler()})
        worker = WritePathWorker(crypto_provider=FakeCrypto(), transport_client=router)
        process = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert process.outcome == 'SENT'
        shift = ShiftRepository.get_active_shift(conn, 'FN-DEV-0001')
        assert shift is not None
        assert shift.state == ShiftState.OPENING

        service = ReconciliationService(transport_status_client=router)
        recon = service.reconcile_pending(conn)
        assert recon.acked == 1
        shift = ShiftRepository.get_active_shift(conn, 'FN-DEV-0001')
        assert shift is not None
        assert shift.state == ShiftState.OPENED
    finally:
        pass  # conn lifecycle owned by conftest fixture


# ---------------------------------------------------------------------------
# Task 9 additions
# ---------------------------------------------------------------------------


def test_reconcile_all_fiscal_numbers_when_fn_is_none(conn):
    """fiscal_number=None must scan all FNs and ACK any SENT doc found."""
    _seed_shift(conn)
    _accept_sell(conn)
    # Process through write_path to create a SENT document
    handler = FakeHandler(
        send_result=SendResult(
            transport_request_id='req-none-fn',
            submission_status='SENT',
            sent_at=datetime.now(UTC),
            response_json='{}',
        ),
        poll_result=PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK',
            ack_at=datetime.now(UTC),
            response_json='{"status": "ACK"}',
        ),
    )
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: handler})
    worker = WritePathWorker(crypto_provider=FakeCrypto(), transport_client=router)
    process_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert process_result.outcome == 'ACK'

    # Reset doc to SENT to simulate crash-before-ACK scenario
    doc = FiscalDocumentRepository.get_by_id(conn, process_result.document_id)
    conn.execute(
        "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL WHERE document_id=?",
        (doc.document_id,),
    )
    conn.commit()

    # Reconcile with fiscal_number=None (global scan)
    svc = ReconciliationService(transport_status_client=router)
    result = svc.reconcile_pending(conn, fiscal_number=None)

    assert result.acked >= 1, f"Expected at least 1 ACK from global scan, got {result}"


def test_rate_limit_cooldown_skips_document(conn):
    """A DPS_RATE_LIMITED doc within its cooldown window must be skipped (still_pending)."""
    _seed_shift(conn)
    _accept_sell(conn)
    handler = FakeHandler(
        send_result=SendResult(
            transport_request_id='req-rl',
            submission_status='SENT',
            sent_at=datetime.now(UTC),
        ),
        poll_result=PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK',
            ack_at=datetime.now(UTC),
            response_json='{}',
        ),
    )
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: handler})
    worker = WritePathWorker(crypto_provider=FakeCrypto(), transport_client=router)
    process_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert process_result.outcome == 'ACK'

    doc_id = process_result.document_id
    # Simulate a rate-limited doc: SENT state but submission_status=DPS_RATE_LIMITED
    # with updated_at=NOW (cooldown window active)
    conn.execute(
        """UPDATE fiscal_documents
           SET state='SENT', ack_at=NULL,
               submission_status='DPS_RATE_LIMITED',
               response_json=?,
               updated_at=CURRENT_TIMESTAMP
           WHERE document_id=?""",
        ('{"retry_after_seconds": 600}', doc_id),
    )
    conn.commit()

    svc = ReconciliationService(transport_status_client=router)
    result = svc.reconcile_pending(conn, fiscal_number='FN-DEV-0001')

    # Document is within cooldown window — must be skipped
    assert result.acked == 0, "Rate-limited doc within cooldown must not be ACK'd"
    assert result.still_pending >= 1, "Skipped doc must appear in still_pending"


def test_reconcile_returns_correct_counts(conn):
    """After reconciliation of 1 SENT doc with ACK poll, checked>=1 and acked>=1."""
    _seed_shift(conn)
    _accept_sell(conn)
    handler = FakeHandler(
        send_result=SendResult(
            transport_request_id='req-counts',
            submission_status='SENT',
            sent_at=datetime.now(UTC),
            response_json='{}',
        ),
        poll_result=PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK',
            ack_at=datetime.now(UTC),
            response_json='{"status": "ACK"}',
        ),
    )
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: handler})
    worker = WritePathWorker(crypto_provider=FakeCrypto(), transport_client=router)
    process_result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert process_result.outcome == 'ACK'

    doc = FiscalDocumentRepository.get_by_id(conn, process_result.document_id)
    conn.execute(
        "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL WHERE document_id=?",
        (doc.document_id,),
    )
    conn.commit()

    svc = ReconciliationService(transport_status_client=router)
    result = svc.reconcile_pending(conn, fiscal_number='FN-DEV-0001')

    assert result.checked >= 1, f"checked must be >= 1, got {result.checked}"
    assert result.acked >= 1, f"acked must be >= 1, got {result.acked}"
    assert result.still_pending == 0, f"still_pending must be 0, got {result.still_pending}"


# ---------------------------------------------------------------------------
# Shift state guard — reconciliation (mirrors offline_sync A3 tests)
# ---------------------------------------------------------------------------

def _recon_fake_doc(doc_type: str) -> object:
    return SimpleNamespace(
        doc_type=doc_type,
        fiscal_number='FN-DEV-0001',
        document_id='doc-recon-guard',
        fs_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        request_id='req-recon-guard',
        ack_at=datetime.now(UTC).isoformat(),
        sent_at=None,
    )


def _create_recon_shift(conn, shift_id: str, state: ShiftState) -> None:
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn,
        shift_id=shift_id,
        fiscal_number='FN-DEV-0001',
        state=state,
        open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST,
        integration_owner='test',
        channel_lock_acquired_at=datetime.now(UTC).isoformat(),
    )
    conn.commit()


def test_recon_shift_closing_not_moved_to_opened(conn, caplog) -> None:
    """Reconciliation: CLOSING shift must NOT be transitioned to OPENED on SHIFT_OPEN ACK."""
    _create_recon_shift(conn, 'shift-recon-closing', ShiftState.CLOSING)

    with caplog.at_level(logging.WARNING, logger='prro_gateway.services.reconciliation'):
        ReconciliationService._apply_shift_side_effects_locked(
            conn, doc=_recon_fake_doc('SHIFT_OPEN'), target_state=DocumentState.ACK,
        )

    shift = ShiftRepository.get_by_id(conn, 'shift-recon-closing')
    assert shift.state == ShiftState.CLOSING, 'CLOSING shift must not be moved to OPENED by reconciliation'
    assert 'reconciliation_shift_invalid_state_for_open' in caplog.text


def test_recon_shift_opening_transitions_to_opened(conn) -> None:
    """Reconciliation: OPENING shift IS correctly transitioned to OPENED on SHIFT_OPEN ACK."""
    _create_recon_shift(conn, 'shift-recon-opening', ShiftState.OPENING)

    ReconciliationService._apply_shift_side_effects_locked(
        conn, doc=_recon_fake_doc('SHIFT_OPEN'), target_state=DocumentState.ACK,
    )

    shift = ShiftRepository.get_by_id(conn, 'shift-recon-opening')
    assert shift.state == ShiftState.OPENED, 'OPENING shift must be transitioned to OPENED'


def test_recon_shift_close_guard_on_opening_shift(conn) -> None:
    """Reconciliation: OPENING shift must NOT be moved to CLOSED on SHIFT_CLOSE ACK."""
    _create_recon_shift(conn, 'shift-recon-close-guard', ShiftState.OPENING)

    ReconciliationService._apply_shift_side_effects_locked(
        conn, doc=_recon_fake_doc('SHIFT_CLOSE'), target_state=DocumentState.ACK,
    )

    shift = ShiftRepository.get_by_id(conn, 'shift-recon-close-guard')
    assert shift.state == ShiftState.OPENING, 'OPENING shift must not be closed by reconciliation SHIFT_CLOSE'
