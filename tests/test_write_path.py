from __future__ import annotations

import sqlite3
import json
from datetime import UTC, datetime, timedelta

from prro_gateway.enums import CanonicalErrorCode, NodeMode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.ports import CryptoProviderUnavailableError, SendResult, TransportRetryableError
from prro_gateway.repositories import InboxRepository, OfflineRepository, ShiftRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json



class StubCryptoProvider:
    def __init__(self, *, fail: bool = False, conn: sqlite3.Connection | None = None) -> None:
        self.fail = fail
        self.conn = conn
        self.calls = 0

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.calls += 1
        if self.conn is not None:
            assert self.conn.in_transaction is False
        if self.fail:
            raise CryptoProviderUnavailableError('crypto down')
        return f'signed::{document_id}::{payload_json}'


class StubTransportClient:
    def __init__(self, *, fail_retryable: bool = False, conn: sqlite3.Connection | None = None) -> None:
        self.fail_retryable = fail_retryable
        self.conn = conn
        self.calls = 0

    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, **kwargs) -> SendResult:
        self.calls += 1
        if self.conn is not None:
            assert self.conn.in_transaction is False
        if self.fail_retryable:
            raise TransportRetryableError('transport timeout')
        now = datetime.now(UTC)
        return SendResult(
            transport_request_id=f'tx-{document_id}',
            submission_status='ACK',
            server_fiscal_no=f'FISC-{document_id}',
            server_fiscal_date=now.isoformat(),
            response_json=json.dumps({'ok': True, 'document_id': document_id}),
            sent_at=now,
            ack_at=now,
        )


class NeverSendTransportClient(StubTransportClient):
    def send(self, **kwargs):  # type: ignore[override]
        raise AssertionError('transport.send() must not be called in offline mode')


class NoSignTransportClient(StubTransportClient):
    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, **kwargs) -> SendResult:
        self.calls += 1
        assert signed_payload == ''
        now = datetime.now(UTC)
        return SendResult(
            state='ACK',
            transport_request_id=f'tx-{document_id}',
            submission_status='ACK',
            server_fiscal_no=f'FISC-{document_id}',
            server_fiscal_date=now.isoformat(),
            response_json=json.dumps({'ok': True, 'document_id': document_id}),
            sent_at=now,
            ack_at=now,
        )



def _accept_command(
    conn: sqlite3.Connection,
    *,
    request_id: str,
    operation_type: OperationType,
    owner: str = 'front-a',
    backend_profile_id: str = 'backend_checkbox_default',
    payload: dict | None = None,
) -> None:
    cmd = CanonicalFiscalCommand(
        request_id=request_id,
        idempotency_key=f'idem-{request_id}',
        protocol=Protocol.CHECKBOX_REST,
        operation_type=operation_type,
        fiscal_number='FN-DEV-0001',
        route_key='main',
        backend_profile_id=backend_profile_id,
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner=owner,
        external_request_id=f'ext-{request_id}',
        business_ts=datetime(2026, 1, 1, 10, 0, 0, tzinfo=UTC),
        payload=payload or {
            'receipt': {
                'type': operation_type.value,
                'goods': [{'name': 'Test item', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': [{'amount': 1000, 'type': 'CASH'}],
                'totals': {'total_sum': 1000},
            },
        },
        payload_sha256=f'sha-{request_id}',
        trace_context=TraceContext(source_ip='10.0.0.10', source_port=12000, session_id='sess-1', correlation_id=f'corr-{request_id}'),
        correlation_id=f'corr-{request_id}',
    )
    InboxRepository.accept_command(
        conn,
        request_id=request_id,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner,
        external_request_id=cmd.external_request_id,
        protocol_session_id='proto-session-1',
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )



def _accept_sell_command(conn: sqlite3.Connection, *, request_id: str = 'req-1', owner: str = 'front-a', backend_profile_id: str = 'backend_checkbox_default', payload: dict | None = None) -> None:
    _accept_command(conn, request_id=request_id, operation_type=OperationType.SELL, owner=owner, backend_profile_id=backend_profile_id, payload=payload)



def _open_shift(conn: sqlite3.Connection, *, owner: str = 'front-a', backend_profile_id: str = 'backend_checkbox_default') -> None:
    ShiftRepository.create_shift(
        conn,
        shift_id='shift-1',
        fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED,
        open_mode='ONLINE',
        backend_profile_id=backend_profile_id,
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST,
        integration_owner=owner,
        channel_lock_acquired_at='2026-01-01T09:59:00+00:00',
    )



def _enable_offline(conn: sqlite3.Connection, *, started_at: str | None = None, accumulated_month_seconds: int = 60, current_month_bucket: str | None = None, current_month_offline_seconds: int = 60) -> None:
    if started_at is None:
        started_at = (datetime.now(UTC) - timedelta(hours=1)).isoformat()
    if current_month_bucket is None:
        current_month_bucket = datetime.now(UTC).strftime('%Y-%m')
    conn.execute(
        "UPDATE node_state SET mode = 'OFFLINE', current_offline_session_id = 'offline-1', current_month_bucket = ?, current_month_offline_seconds = ? WHERE fiscal_number = 'FN-DEV-0001'",
        (current_month_bucket, current_month_offline_seconds),
    )
    OfflineRepository.create_open_session(
        conn,
        offline_session_id='offline-1',
        fiscal_number='FN-DEV-0001',
        started_at=started_at,
        accumulated_month_seconds=accumulated_month_seconds,
    )
    conn.execute(
        """
        INSERT INTO offline_ranges (range_id, fiscal_number, first_fiscal_no, last_fiscal_no, next_fiscal_no, issued_at, status)
        VALUES ('range-1', 'FN-DEV-0001', 1000, 1005, 1000, '2026-01-01T09:00:00+00:00', 'ACTIVE')
        """
    )



def test_worker_sell_happy_path_ack_and_outbox_and_artifacts(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn)
        conn.commit()

        crypto = StubCryptoProvider(conn=conn)
        transport = StubTransportClient(conn=conn)
        worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ACK'
        assert result.inbox_status == 'DONE'
        assert result.document_state == 'ACK'
        row = conn.execute('SELECT COUNT(*) FROM sync_outbox WHERE aggregate_id = ?', (result.document_id,)).fetchone()
        assert row[0] == 1
        files = conn.execute('SELECT file_kind FROM document_files WHERE document_id = ? ORDER BY file_kind', (result.document_id,)).fetchall()
        assert {row[0] for row in files} == {'REQUEST_JSON', 'SIGNED_XML', 'PROTO_RESPONSE'}
        assert conn.execute('SELECT COUNT(*) FROM audit_log WHERE entity_id = ?', (result.document_id,)).fetchone()[0] >= 3
        assert conn.execute('SELECT COUNT(*) FROM protocol_trace_log WHERE canonical_request_id = ?', ('req-1',)).fetchone()[0] >= 2
        assert conn.execute('SELECT COUNT(*) FROM transport_trace_log WHERE fiscal_number = ?', ('FN-DEV-0001',)).fetchone()[0] == 2
        assert crypto.calls == 1
        assert transport.calls == 1
    finally:
        conn.close()



def test_worker_rejects_channel_switch(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn, owner='front-a')
        _accept_sell_command(conn, owner='front-b', request_id='req-2')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.SHIFT_CHANNEL_SWITCH_FORBIDDEN
        inbox = conn.execute('SELECT status, canonical_error_code FROM ingress_inbox WHERE request_id = ?', ('req-2',)).fetchone()
        assert tuple(inbox) == ('ERROR', 'SHIFT_CHANNEL_SWITCH_FORBIDDEN')
    finally:
        conn.close()



def test_worker_requires_open_shift_for_sell(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _accept_sell_command(conn, request_id='req-3')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.SHIFT_NOT_OPEN
    finally:
        conn.close()



def test_worker_marks_crypto_failure_retryable(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-4')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(fail=True, conn=conn), transport_client=StubTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE
        doc = conn.execute('SELECT state, canonical_error_code FROM fiscal_documents').fetchone()
        assert tuple(doc) == ('ERROR_RETRYABLE', 'CRYPTO_PROVIDER_UNAVAILABLE')
    finally:
        conn.close()



def test_worker_marks_transport_failure_retryable(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-5')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(fail_retryable=True, conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR
        doc = conn.execute('SELECT state, canonical_error_code FROM fiscal_documents').fetchone()
        assert tuple(doc) == ('ERROR_RETRYABLE', 'TRANSPORT_RETRYABLE_ERROR')
    finally:
        conn.close()



def test_worker_capability_gate_cash_withdrawal(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        conn.execute(
            """
            INSERT INTO backend_profiles (backend_profile_id, backend_type, name, capability_flags_json, config_json, is_active)
            VALUES ('backend_no_cash', 'CUSTOM_VENDOR_BACKEND', 'No Cash Backend', json('{"supports_cash_withdrawal": false, "supports_service_receipts": true, "supports_x_report": true}'), json('{}'), 1)
            """
        )
        _open_shift(conn, backend_profile_id='backend_no_cash')
        _accept_command(conn, request_id='req-6', operation_type=OperationType.CASH_WITHDRAWAL, backend_profile_id='backend_no_cash', payload={'receipt': {'type': 'CASH_WITHDRAWAL'}})
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.UNSUPPORTED_BY_BACKEND
    finally:
        conn.close()



# ---------------------------------------------------------------------------
# C1: CRYPTO_DEGRADED fast-path guard
#
# When node.mode == CRYPTO_DEGRADED, all fiscal operations must be rejected
# immediately (before LND allocation) with CRYPTO_PROVIDER_UNAVAILABLE.
# Management operations (GET_STATUS, GO_ONLINE) must still pass through.
# ---------------------------------------------------------------------------

def test_worker_crypto_degraded_rejects_fiscal_ops_without_lnd(conn):
    """CRYPTO_DEGRADED mode: SELL is rejected before any document or LND is created."""
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-crypto-degraded')
        conn.execute("UPDATE node_state SET mode = 'CRYPTO_DEGRADED' WHERE fiscal_number = 'FN-DEV-0001'")
        conn.commit()

        # Threshold=3, failures=3 → breaker is open. This mirrors real CRYPTO_DEGRADED state.
        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn), crypto_breaker_threshold=3)
        worker._crypto_consecutive_failures = 3
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE

        # No LND consumed — no fiscal_document must exist
        doc_count = conn.execute('SELECT COUNT(*) FROM fiscal_documents').fetchone()[0]
        assert doc_count == 0, "CRYPTO_DEGRADED must not allocate LND or create a document"

        # Inbox row must be marked ERROR
        row = conn.execute(
            "SELECT status, canonical_error_code FROM ingress_inbox WHERE request_id = 'req-crypto-degraded'"
        ).fetchone()
        assert row is not None
        assert row[0] == 'ERROR'
        assert row[1] == 'CRYPTO_PROVIDER_UNAVAILABLE'
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# H3: BLOCKED + GO_OFFLINE returns correct error code OFFLINE_LIMIT_REACHED
# (not the misleading NODE_ALREADY_OFFLINE)
# ---------------------------------------------------------------------------

def test_worker_blocked_go_offline_returns_offline_limit_reached(conn):
    """When node is BLOCKED, GO_OFFLINE must return OFFLINE_LIMIT_REACHED, not NODE_ALREADY_OFFLINE."""
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_command(conn, request_id='req-blocked-go-offline', operation_type=OperationType.GO_OFFLINE,
                        payload={'receipt': {'type': 'GO_OFFLINE'}})
        conn.execute("UPDATE node_state SET mode = 'BLOCKED' WHERE fiscal_number = 'FN-DEV-0001'")
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.OFFLINE_LIMIT_REACHED, (
            f"Expected OFFLINE_LIMIT_REACHED, got {result.canonical_error.code}"
        )
    finally:
        conn.close()


def test_accept_command_returns_existing_on_duplicate_idempotency_key(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        first, first_replay = InboxRepository.accept_command(
            conn,
            request_id='dup-1',
            idempotency_key='idem-dup',
            protocol=Protocol.CHECKBOX_REST,
            operation_type=OperationType.SELL,
            fiscal_number='FN-DEV-0001',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            channel_owner='front-a',
            payload_json=dumps_json({'request_id': 'dup-1'}),
            payload_sha256='sha-dup-1',
        )
        second, second_replay = InboxRepository.accept_command(
            conn,
            request_id='dup-2',
            idempotency_key='idem-dup',
            protocol=Protocol.CHECKBOX_REST,
            operation_type=OperationType.SELL,
            fiscal_number='FN-DEV-0001',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            channel_owner='front-a',
            payload_json=dumps_json({'request_id': 'dup-2'}),
            payload_sha256='sha-dup-2',
        )
        conn.commit()
        assert first.request_id == 'dup-1'
        assert first_replay is False
        assert second.request_id == 'dup-1'
        assert second_replay is True
        row = conn.execute('SELECT COUNT(*) FROM ingress_inbox WHERE idempotency_key = ?', ('idem-dup',)).fetchone()
        assert row[0] == 1
    finally:
        conn.close()



def test_worker_external_calls_happen_outside_transaction_and_sets_sent_timestamps(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-7')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ACK'
        row = conn.execute('SELECT sent_at, ack_at, submission_status, transport_request_id FROM fiscal_documents WHERE document_id = ?', (result.document_id,)).fetchone()
        assert row[0] is not None
        assert row[1] is not None
        assert row[2] == 'ACK'
        assert row[3] == f'tx-{result.document_id}'
    finally:
        conn.close()





def test_worker_offline_sell_rejects_when_continuous_limit_exceeded(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _enable_offline(
            conn,
            started_at='2025-12-30T00:00:00+00:00',
            accumulated_month_seconds=60,
            current_month_bucket='2026-01',
            current_month_offline_seconds=60,
        )
        _accept_sell_command(conn, request_id='req-8b')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=NeverSendTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.OFFLINE_LIMIT_REACHED
        doc_count = conn.execute('SELECT COUNT(*) FROM fiscal_documents').fetchone()[0]
        assert doc_count == 0
    finally:
        conn.close()



def test_worker_offline_sell_rejects_when_month_limit_exceeded(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _enable_offline(
            conn,
            started_at='2026-01-15T10:00:00+00:00',
            accumulated_month_seconds=168 * 60 * 60,
            current_month_bucket='2026-01',
            current_month_offline_seconds=168 * 60 * 60,
        )
        _accept_sell_command(conn, request_id='req-8c')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=NeverSendTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ERROR'
        assert result.canonical_error is not None
        assert result.canonical_error.code == CanonicalErrorCode.OFFLINE_LIMIT_REACHED
        doc_count = conn.execute('SELECT COUNT(*) FROM fiscal_documents').fetchone()[0]
        assert doc_count == 0
    finally:
        conn.close()

def test_worker_offline_sell_allocates_offline_code_and_skips_transport(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _enable_offline(conn)
        _accept_sell_command(conn, request_id='req-8')
        conn.commit()

        crypto = StubCryptoProvider(conn=conn)
        transport = NeverSendTransportClient(conn=conn)
        worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'OFFLINE_LOCAL_ACK'
        row = conn.execute('SELECT fs_mode, offline_fiscal_no, offline_session_id, submission_status FROM fiscal_documents WHERE document_id = ?', (result.document_id,)).fetchone()
        assert tuple(row) == ('OFFLINE', 1000, 'offline-1', 'OFFLINE_LOCAL')
        rng = conn.execute("SELECT next_fiscal_no, status FROM offline_ranges WHERE range_id = 'range-1'").fetchone()
        assert tuple(rng) == (1001, 'ACTIVE')
        assert crypto.calls == 1
    finally:
        conn.close()



def test_worker_duplicate_excise_mark_blocks_second_sell(conn):
    try:
        payload = {
            'receipt': {
                'type': 'SELL',
                'goods': [
                    {'item_no': 1, 'code': 'ALC001', 'uktzed': '2208', 'excise_barcodes': ['ABCD000123'], 'name': 'Alcohol', 'price': 5000, 'quantity': 1000, 'sum': 5000},
                ],
                'payments': [{'amount': 5000, 'type': 'CASH'}],
                'totals': {'total_sum': 5000},
            }
        }
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-9a', payload=payload)
        _accept_sell_command(conn, request_id='req-9b', payload=payload)
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn))
        first = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert first.outcome == 'ACK'
        second = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert second.outcome == 'ERROR'
        assert second.canonical_error is not None
        assert second.canonical_error.code == CanonicalErrorCode.DUPLICATE_EXCISE_MARK
        marks = conn.execute('SELECT normalized_mark_code, status, receipt_document_id FROM excise_marks').fetchall()
        assert [tuple(r) for r in marks] == [('ABCD000123', 'SOLD', first.document_id)]
    finally:
        conn.close()



def test_worker_records_protocol_trace_context_fields(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-10')
        conn.commit()

        worker = WritePathWorker(crypto_provider=StubCryptoProvider(conn=conn), transport_client=StubTransportClient(conn=conn))
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ACK'
        row = conn.execute(
            "SELECT source_ip, source_port, session_id, result_code FROM protocol_trace_log WHERE canonical_request_id = ? AND result_code = 'ACK' LIMIT 1",
            ('req-10',),
        ).fetchone()
        assert tuple(row) == ('10.0.0.10', 12000, 'sess-1', 'ACK')
    finally:
        conn.close()


def test_worker_can_skip_local_sign_when_transport_profile_disables_it(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-skip-sign')
        conn.execute(
            "UPDATE transport_profiles SET config_json = ? WHERE transport_profile_id = 'transport_checkbox_rest_default'",
            (json.dumps({'headers': {'X-Client-Name': 'test', 'X-Client-Version': '1.0'}, 'require_local_sign': False}),),
        )
        conn.commit()

        crypto = StubCryptoProvider(conn=conn)
        transport = NoSignTransportClient(conn=conn)
        worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ACK'
        assert crypto.calls == 0
        files = conn.execute('SELECT file_kind FROM document_files WHERE document_id = ? ORDER BY file_kind', (result.document_id,)).fetchall()
        assert {row[0] for row in files} == {'REQUEST_JSON', 'PROTO_RESPONSE'}
    finally:
        conn.close()


def test_worker_shift_open_creates_opened_shift_on_ack(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _accept_command(conn, request_id='req-shift-open', operation_type=OperationType.SHIFT_OPEN, payload={'receipt': {'type': 'SHIFT_OPEN'}})
        conn.execute(
            "UPDATE transport_profiles SET config_json = ? WHERE transport_profile_id = 'transport_checkbox_rest_default'",
            (json.dumps({'require_local_sign': False}),),
        )
        conn.commit()

        crypto = StubCryptoProvider(conn=conn)
        transport = NoSignTransportClient(conn=conn)
        worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ACK'
        shift = ShiftRepository.get_active_shift(conn, 'FN-DEV-0001')
        assert shift is not None
        assert shift.state == ShiftState.OPENED
        assert shift.opened_via_backend_profile_id == 'backend_checkbox_default'
        assert crypto.calls == 0
    finally:
        conn.close()


def test_worker_shift_close_marks_active_shift_closed_on_ack(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_command(conn, request_id='req-shift-close', operation_type=OperationType.SHIFT_CLOSE, payload={'receipt': {'type': 'SHIFT_CLOSE'}})
        conn.execute(
            "UPDATE transport_profiles SET config_json = ? WHERE transport_profile_id = 'transport_checkbox_rest_default'",
            (json.dumps({'require_local_sign': False}),),
        )
        conn.commit()

        crypto = StubCryptoProvider(conn=conn)
        transport = NoSignTransportClient(conn=conn)
        worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)
        result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        assert result.outcome == 'ACK'
        shift = conn.execute("SELECT state FROM shifts WHERE shift_id = 'shift-1'").fetchone()
        assert tuple(shift) == ('CLOSED',)
        assert crypto.calls == 0
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# C2: crash-resume when document is already in SIGNED state
#
# Scenario: process crashed after SIGNED commit but before SENT commit.
# On retry, _stage_sign must be skipped (no new LND, no re-sign call),
# the persisted signed artifact must be reused, and the document must
# proceed to send → ACK exactly once.
# ---------------------------------------------------------------------------

def test_worker_signed_crash_resume_skips_resign_and_sends(conn):
    """After a crash-resume where document.state == SIGNED:
    - crypto.sign() must NOT be called again
    - transport.send() must be called exactly once
    - result.outcome must be ACK
    - exactly one fiscal_document in DB (no duplicate LND)
    """
    import uuid
    from prro_gateway.enums import DocumentState, FileKind
    from prro_gateway.repositories.fiscal_documents import FiscalDocumentRepository
    from prro_gateway.repositories.document_files import DocumentFilesRepository
    from prro_gateway.repositories.node_state import NodeStateRepository

    try:
        conn.execute('BEGIN IMMEDIATE')
        _open_shift(conn)
        _accept_sell_command(conn, request_id='req-signed-resume')
        conn.commit()

        # Step 1: first pass — sign succeeds, transport will fail (retryable)
        #         → leaves document in SIGNED state after crash simulation
        crypto = StubCryptoProvider(conn=conn)
        transport_fail = StubTransportClient(fail_retryable=True, conn=conn)
        worker = WritePathWorker(crypto_provider=crypto, transport_client=transport_fail)
        result1 = worker.process_next(conn, fiscal_number='FN-DEV-0001')
        # Transport failed → document should be ERROR_RETRYABLE
        assert result1.outcome == 'ERROR'
        assert crypto.calls == 1  # signed once

        # Simulate crash-after-SIGNED: manually set document state back to SIGNED
        # (this mimics what would happen if the process crashed between SIGNED
        # commit and SENT commit — transport hadn't even been called yet)
        doc_id = result1.document_id
        assert doc_id is not None
        conn.execute(
            "UPDATE fiscal_documents SET state = 'SIGNED', transport_request_id = NULL WHERE document_id = ?",
            (doc_id,),
        )
        conn.execute(
            "UPDATE ingress_inbox SET status = 'NEW', canonical_error_code = NULL, requeue_count = 0 WHERE request_id = 'req-signed-resume'",
        )
        conn.commit()

        # Step 2: retry — crypto must NOT be called, transport must succeed
        crypto2 = StubCryptoProvider(conn=conn)  # fresh counter
        transport_ok = StubTransportClient(conn=conn)
        worker2 = WritePathWorker(crypto_provider=crypto2, transport_client=transport_ok)
        result2 = worker2.process_next(conn, fiscal_number='FN-DEV-0001')

        assert result2.outcome == 'ACK', f"Expected ACK, got {result2.outcome}: {result2.canonical_error}"
        assert crypto2.calls == 0, "crypto.sign() must not be called on SIGNED-state resume"
        assert transport_ok.calls == 1, "transport.send() must be called exactly once on resume"

        # Exactly one fiscal_document (no duplicate LND allocated)
        doc_count = conn.execute('SELECT COUNT(*) FROM fiscal_documents').fetchone()[0]
        assert doc_count == 1, f"Expected 1 fiscal_document, got {doc_count} (duplicate LND!)"

        assert result2.document_id == doc_id, "Must reuse existing document_id, not create a new one"
    finally:
        conn.close()
