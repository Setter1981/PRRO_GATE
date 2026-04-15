from __future__ import annotations


from prro_gateway.enums import CanonicalErrorCode, InboxStatus, NodeMode, Protocol, ShiftState
from prro_gateway.errors import build_canonical_error
from prro_gateway.repositories import FiscalDocumentRepository, InboxRepository, NodeStateRepository, ShiftRepository



def test_inbox_lease_acquire_release(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            conn,
            request_id='req-1',
            idempotency_key='idem-1',
            protocol=Protocol.CHECKBOX_REST,
            operation_type='SELL',
            fiscal_number='FN-DEV-0001',
            payload_json='{}',
            payload_sha256='sha1',
        )
        acquired = InboxRepository.acquire_lease(
            conn,
            fiscal_number='FN-DEV-0001',
            lease_owner='worker-1',
            lease_token='lease-1',
            lease_expires_at='2999-01-01T00:00:00',
        )
        assert acquired is not None
        assert acquired.status == InboxStatus.PROCESSING
        assert acquired.lease_token == 'lease-1'
        released = InboxRepository.release_lease(conn, request_id='req-1', lease_token='lease-1')
        assert released is not None
        assert released.lease_token is None
        conn.commit()
    finally:
        conn.close()


def test_inbox_idempotency_duplicate_returns_existing_record(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        first, first_replay = InboxRepository.accept_command(
            conn,
            request_id='req-1',
            idempotency_key='idem-dup',
            protocol=Protocol.CHECKBOX_REST,
            operation_type='SELL',
            fiscal_number='FN-DEV-0001',
            payload_json='{}',
            payload_sha256='sha1',
        )
        conn.commit()
        conn.execute('BEGIN IMMEDIATE')
        second, second_replay = InboxRepository.accept_command(
            conn,
            request_id='req-2',
            idempotency_key='idem-dup',
            protocol=Protocol.CHECKBOX_REST,
            operation_type='SELL',
            fiscal_number='FN-DEV-0001',
            payload_json='{}',
            payload_sha256='sha2',
        )
        conn.commit()
        assert first.request_id == 'req-1'
        assert first_replay is False
        assert second.request_id == 'req-1'
        assert second_replay is True
        row = conn.execute("SELECT COUNT(*) FROM ingress_inbox WHERE idempotency_key = ?", ('idem-dup',)).fetchone()
        assert row[0] == 1
    finally:
        conn.close()


def test_shift_channel_lock_read(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        created = ShiftRepository.create_shift(
            conn,
            shift_id='shift-1',
            fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED,
            open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            protocol=Protocol.CHECKBOX_REST,
            integration_owner='front-a',
            channel_lock_acquired_at='2026-01-01T10:00:00',
        )
        lock = ShiftRepository.get_channel_lock(conn, 'FN-DEV-0001')
        assert created.state == ShiftState.OPENED
        assert lock is not None
        assert lock.backend_profile_id == 'backend_checkbox_default'
        assert lock.protocol == Protocol.CHECKBOX_REST
        conn.commit()
    finally:
        conn.close()


def test_node_state_lnd_increment_atomic(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        first = NodeStateRepository.increment_lnd(conn, fiscal_number='FN-DEV-0001')
        second = NodeStateRepository.increment_lnd(conn, fiscal_number='FN-DEV-0001')
        state = NodeStateRepository.get_state(conn, 'FN-DEV-0001')
        assert first == 1
        assert second == 2
        assert state is not None
        assert state.next_lnd == 3
        conn.commit()
    finally:
        conn.close()


def test_canonical_error_map():
    err = build_canonical_error(CanonicalErrorCode.WORKER_BUSY_TIMEOUT)
    assert err.retryable is True
    assert err.code == CanonicalErrorCode.WORKER_BUSY_TIMEOUT
    assert 'Worker busy timeout' in err.message


def test_fiscal_document_create_and_read(conn):
    try:
        conn.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            conn,
            request_id='req-doc-1',
            idempotency_key='idem-doc-1',
            protocol=Protocol.CHECKBOX_REST,
            operation_type='SELL',
            fiscal_number='FN-DEV-0001',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            payload_json='{"demo":true}',
            payload_sha256='sha-doc-1',
        )
        record = FiscalDocumentRepository.create_prepared(
            conn,
            document_id='doc-1',
            request_id='req-doc-1',
            fiscal_number='FN-DEV-0001',
            lnd=1,
            doc_type='SELL',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            fs_mode='ONLINE',
            business_ts='2026-01-01T10:00:00',
            payload_json='{"demo":true}',
            payload_sha256='sha-doc-1',
            previous_hash='prev-hash-1',
        )
        loaded = FiscalDocumentRepository.get_by_id(conn, 'doc-1')
        assert record.document_id == 'doc-1'
        assert record.state.value == 'PREPARED'
        assert record.previous_hash == 'prev-hash-1'
        assert record.offline_fiscal_no is None
        assert loaded is not None
        assert loaded.document_id == 'doc-1'
        assert loaded.payload_sha256 == 'sha-doc-1'
        assert loaded.recovery_attempts == 0
        conn.commit()
    finally:
        conn.close()
