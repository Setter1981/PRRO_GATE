"""
Sprint 2 / bounded step 2 — OfflineSyncService.sync_pending() unit tests.

Uses in-memory SQLite (conftest conn fixture) + mock transport implementations.
No HTTP stack is required — the service takes a TransportClient protocol object.

Coverage matrix:
  T1 — only offline candidates processed: online SENT doc ignored
  T2 — ordered processing: docs sent in offline_fiscal_no ASC order
  T3 — success → ACK: state, ack_at, server_fiscal_no, response_json written
  T3b — ACK enqueues outbox: sync_outbox row created on ACK (parity with reconciliation)
  T4 — async DPS → SENT: transport_request_id stored; reconciliation can poll
  T5 — retryable failure: stays OFFLINE_LOCAL_ACK, recovery_attempts incremented
  T6 — terminal failure (TransportRejectedError) → REQUIRES_MANUAL_RECONCILIATION
  T7 — max retries exhausted → REQUIRES_MANUAL_RECONCILIATION
  T-rejected — SendResult.state='REJECTED' → REJECTED (business rejection, terminal)
  T-unknown — unrecognised DPS state → retryable, connection stays clean
  T-unexpected-ceiling — valid-but-unexpected state escalates after max retries
  T8 — reconciliation queue unchanged: OFFLINE_LOCAL_ACK disjoint from SENT queue

Invariants verified:
  #1  transport.send() called OUTSIDE transaction — structural (transport call
      precedes BEGIN IMMEDIATE in code flow; not runtime-asserted).
  #4  expected_states guard — structural (all update_state calls pass
      expected_states=(OFFLINE_LOCAL_ACK,)); no concurrent-modification test.
  #8  Every outcome is persisted with an audit event (checked in T3, T5, T6,
      T-rejected).
"""
from __future__ import annotations

import itertools
import uuid
from datetime import UTC, datetime

from prro_gateway.enums import DocumentState
from prro_gateway.ports import SendResult, TransportRejectedError, TransportRetryableError
from prro_gateway.repositories.fiscal_documents import FiscalDocumentRepository
from prro_gateway.services.offline_sync import OfflineSyncRunResult, OfflineSyncService

# ---------------------------------------------------------------------------
# Constants (same profiles as seeded by migration 002)
# ---------------------------------------------------------------------------

_FN = 'FN-DEV-0001'
_BP = 'backend_checkbox_default'
_TP = 'transport_checkbox_rest_default'

_lnd_counter = itertools.count(1)


def _next_lnd() -> int:
    return next(_lnd_counter)


# ---------------------------------------------------------------------------
# Minimal DB helpers (mirror of test_sprint2_offline_sync_selector.py)
# ---------------------------------------------------------------------------

def _seed_inbox(conn, req_id: str, fiscal_number: str, doc_type: str) -> None:
    conn.execute(
        """
        INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256)
        VALUES (?, ?, 'CHECKBOX_REST', ?, ?, '{}', ?)
        """,
        (req_id, f'{doc_type.lower()}:{fiscal_number.lower()}:{req_id}',
         doc_type, fiscal_number, 'sha-' + req_id[:8]),
    )


def _insert_doc(
    conn,
    *,
    fiscal_number: str = _FN,
    lnd: int | None = None,
    doc_type: str = 'SELL',
    fs_mode: str = 'OFFLINE',
    offline_fiscal_no: int | None = None,
    target_state: DocumentState = DocumentState.OFFLINE_LOCAL_ACK,
    submission_status: str | None = 'OFFLINE_LOCAL',
    payload_json: str = '{}',
) -> str:
    doc_id = str(uuid.uuid4())
    req_id = str(uuid.uuid4())
    effective_lnd = lnd if lnd is not None else _next_lnd()

    _seed_inbox(conn, req_id, fiscal_number, doc_type)
    FiscalDocumentRepository.create_prepared(
        conn,
        document_id=doc_id,
        request_id=req_id,
        fiscal_number=fiscal_number,
        lnd=effective_lnd,
        doc_type=doc_type,
        backend_profile_id=_BP,
        transport_profile_id=_TP,
        fs_mode=fs_mode,
        business_ts='2026-03-29T12:00:00Z',
        payload_json=payload_json,
        payload_sha256='sha-' + doc_id[:8],
        offline_fiscal_no=offline_fiscal_no,
    )
    if target_state != DocumentState.PREPARED:
        FiscalDocumentRepository.update_state(
            conn,
            document_id=doc_id,
            state=target_state,
            submission_status=submission_status,
        )
    conn.commit()
    return doc_id


# ---------------------------------------------------------------------------
# Transport mocks
# ---------------------------------------------------------------------------

class _AckTransport:
    """Always returns immediate ACK from DPS."""

    def __init__(self):
        self.calls: list[str] = []  # document_ids in call order

    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str,
             backend_profile_id: str, transport_profile_id: str,
             operation_type: str | None = None,
             request_payload: dict | None = None,
             request_payload_json: str | None = None,
             external_request_id: str | None = None,
             transport_profile=None, **kwargs) -> SendResult:
        self.calls.append(document_id)
        return SendResult(
            state=DocumentState.ACK.value,
            transport_request_id=f'tr-{document_id[:8]}',
            submission_status='DONE',
            server_fiscal_no=f'SFN-{document_id[:6]}',
            server_fiscal_date='2026-03-29T12:01:00+00:00',
            response_json='{"status":"DONE","fiscal_code":"SFN"}',
            sent_at=datetime.now(UTC),
            ack_at=datetime.now(UTC),
        )


class _SentTransport:
    """DPS accepts async; returns SENT with transport_request_id."""

    def __init__(self):
        self.calls: list[str] = []

    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str,
             backend_profile_id: str, transport_profile_id: str,
             operation_type: str | None = None,
             request_payload: dict | None = None,
             request_payload_json: str | None = None,
             external_request_id: str | None = None,
             transport_profile=None, **kwargs) -> SendResult:
        self.calls.append(document_id)
        return SendResult(
            state=DocumentState.SENT.value,
            transport_request_id=f'tr-async-{document_id[:8]}',
            submission_status='PROCESSING',
            sent_at=datetime.now(UTC),
        )


class _RetryableTransport:
    """Always raises TransportRetryableError."""

    def __init__(self):
        self.calls: int = 0

    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str,
             backend_profile_id: str, transport_profile_id: str,
             operation_type: str | None = None,
             request_payload: dict | None = None,
             request_payload_json: str | None = None,
             external_request_id: str | None = None,
             transport_profile=None, **kwargs) -> SendResult:
        self.calls += 1
        raise TransportRetryableError('network blip')


class _RejectedTransport:
    """Always raises TransportRejectedError (terminal)."""

    def __init__(self):
        self.calls: int = 0

    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str,
             backend_profile_id: str, transport_profile_id: str,
             operation_type: str | None = None,
             request_payload: dict | None = None,
             request_payload_json: str | None = None,
             external_request_id: str | None = None,
             transport_profile=None, **kwargs) -> SendResult:
        self.calls += 1
        raise TransportRejectedError('DPS rejected: invalid fiscal data')


# ---------------------------------------------------------------------------
# T1 — only offline candidates processed
# ---------------------------------------------------------------------------

def test_t1_only_offline_candidates_processed(conn) -> None:
    """Online SENT doc must not be touched; only OFFLINE_LOCAL_ACK processed."""
    transport = _AckTransport()

    online_id = _insert_doc(
        conn,
        fs_mode='ONLINE',
        offline_fiscal_no=None,
        target_state=DocumentState.SENT,
        submission_status=None,
    )
    offline_id = _insert_doc(conn, offline_fiscal_no=1001)

    svc = OfflineSyncService(transport_client=transport)
    result = svc.sync_pending(conn)

    assert result.checked == 1, f'Only 1 offline doc must be checked, got {result.checked}'
    assert result.synced == 1

    # Online doc untouched
    online_doc = FiscalDocumentRepository.get_by_id(conn, online_id)
    assert online_doc.state == DocumentState.SENT, (
        f'Online doc must remain SENT, got {online_doc.state}'
    )

    # Offline doc ACKed
    offline_doc = FiscalDocumentRepository.get_by_id(conn, offline_id)
    assert offline_doc.state == DocumentState.ACK, (
        f'Offline doc must be ACK after sync, got {offline_doc.state}'
    )

    assert transport.calls == [offline_id], (
        f'transport.send() must be called exactly once for offline doc, got {transport.calls}'
    )


# ---------------------------------------------------------------------------
# T2 — ordered processing: offline_fiscal_no ASC
# ---------------------------------------------------------------------------

def test_t2_ordered_processing_fiscal_no_asc(conn) -> None:
    """sync_pending() must call transport.send() in offline_fiscal_no ASC order."""
    transport = _AckTransport()

    id_3000 = _insert_doc(conn, offline_fiscal_no=3000)
    id_1000 = _insert_doc(conn, offline_fiscal_no=1000)
    id_2000 = _insert_doc(conn, offline_fiscal_no=2000)

    svc = OfflineSyncService(transport_client=transport)
    result = svc.sync_pending(conn)

    assert result.synced == 3
    assert transport.calls == [id_1000, id_2000, id_3000], (
        f'Expected fiscal_no order [1000,2000,3000], got {transport.calls}'
    )


# ---------------------------------------------------------------------------
# T3 — success → ACK: fields written correctly
# ---------------------------------------------------------------------------

def test_t3_success_ack_fields(conn) -> None:
    """After successful sync, document must have ACK state with all DPS fields."""
    transport = _AckTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=1001)

    svc = OfflineSyncService(transport_client=transport)
    result = svc.sync_pending(conn)

    assert result.synced == 1
    assert result.checked == 1

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc is not None
    assert doc.state == DocumentState.ACK, f'state must be ACK, got {doc.state}'
    assert doc.ack_at is not None, 'ack_at must be set'
    assert doc.sent_at is not None, 'sent_at must be set'
    assert doc.server_fiscal_no is not None, 'server_fiscal_no must be set'
    assert doc.transport_request_id is not None, 'transport_request_id must be set'
    assert doc.response_json is not None, 'response_json must be set'
    assert doc.submission_status == 'DONE', f'submission_status must be DONE, got {doc.submission_status!r}'

    # Audit event persisted
    audit = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id = ? ORDER BY rowid DESC LIMIT 1",
        (doc_id,),
    ).fetchone()
    assert audit is not None
    assert audit[0] == 'OFFLINE_SYNC_ACK', f'Unexpected audit event: {audit[0]!r}'


# ---------------------------------------------------------------------------
# T4 — async DPS → SENT: transport_request_id stored; reconciliation can poll
# ---------------------------------------------------------------------------

def test_t4_async_dps_sent(conn) -> None:
    """When DPS returns SENT, document transitions to SENT with transport_request_id."""
    transport = _SentTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=2001)

    svc = OfflineSyncService(transport_client=transport)
    result = svc.sync_pending(conn)

    assert result.pending_async == 1, f'Expected pending_async=1, got {result.pending_async}'
    assert result.synced == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.SENT, f'state must be SENT, got {doc.state}'
    assert doc.transport_request_id is not None, 'transport_request_id must be set for reconciliation'
    assert doc.sent_at is not None, 'sent_at must be set'

    # Document is now in reconciliation queue (disjoint selector)
    recon_docs = FiscalDocumentRepository.get_pending_for_reconciliation(conn)
    recon_ids = {r.document_id for r in recon_docs}
    assert doc_id in recon_ids, 'SENT doc must appear in reconciliation queue'

    # No longer in offline sync queue
    sync_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    sync_ids = {r.document_id for r in sync_docs}
    assert doc_id not in sync_ids, 'SENT doc must NOT remain in offline sync queue'


# ---------------------------------------------------------------------------
# T5 — retryable failure: stays OFFLINE_LOCAL_ACK, recovery_attempts++
# ---------------------------------------------------------------------------

def test_t5_retryable_failure_increments_attempts(conn) -> None:
    """TransportRetryableError must keep state OFFLINE_LOCAL_ACK and increment recovery_attempts."""
    transport = _RetryableTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=3001)

    svc = OfflineSyncService(transport_client=transport, max_recovery_attempts=5)
    result = svc.sync_pending(conn)

    assert result.retryable == 1, f'Expected retryable=1, got {result.retryable}'
    assert result.synced == 0
    assert result.manual == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.OFFLINE_LOCAL_ACK, (
        f'Retryable failure must keep OFFLINE_LOCAL_ACK, got {doc.state}'
    )
    assert doc.recovery_attempts == 1, (
        f'recovery_attempts must be 1 after first failure, got {doc.recovery_attempts}'
    )

    # Still in offline sync queue — will be retried next run
    sync_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    assert any(r.document_id == doc_id for r in sync_docs), (
        'Doc must remain in offline sync queue after retryable failure'
    )

    # Audit event persisted
    audit = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id = ? ORDER BY rowid DESC LIMIT 1",
        (doc_id,),
    ).fetchone()
    assert audit is not None
    assert audit[0] == 'OFFLINE_SYNC_FAILED_RETRYABLE', f'Unexpected audit event: {audit[0]!r}'


# ---------------------------------------------------------------------------
# T6 — terminal failure: TransportRejectedError → REQUIRES_MANUAL_RECONCILIATION
# ---------------------------------------------------------------------------

def test_t6_terminal_failure_requires_manual(conn) -> None:
    """TransportRejectedError must move document to REQUIRES_MANUAL_RECONCILIATION immediately."""
    transport = _RejectedTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=4001)

    svc = OfflineSyncService(transport_client=transport, max_recovery_attempts=5)
    result = svc.sync_pending(conn)

    assert result.manual == 1, f'Expected manual=1, got {result.manual}'
    assert result.retryable == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION, (
        f'Terminal failure must move to REQUIRES_MANUAL_RECONCILIATION, got {doc.state}'
    )
    assert doc.error_message is not None, 'error_message must be set for terminal failure'

    # No longer in offline sync queue
    sync_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    assert not any(r.document_id == doc_id for r in sync_docs), (
        'Terminal doc must NOT remain in offline sync queue'
    )

    # Audit event
    audit = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id = ? ORDER BY rowid DESC LIMIT 1",
        (doc_id,),
    ).fetchone()
    assert audit is not None
    assert audit[0] == 'OFFLINE_SYNC_FAILED_TERMINAL', f'Unexpected audit event: {audit[0]!r}'


# ---------------------------------------------------------------------------
# T7 — max retries exhausted → REQUIRES_MANUAL_RECONCILIATION
# ---------------------------------------------------------------------------

def test_t7_max_retries_exhausted_requires_manual(conn) -> None:
    """After max_recovery_attempts retries, must escalate to REQUIRES_MANUAL_RECONCILIATION."""
    transport = _RetryableTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=5001)
    MAX = 3
    svc = OfflineSyncService(transport_client=transport, max_recovery_attempts=MAX)

    # Run MAX-1 times: stays retryable
    for i in range(MAX - 1):
        result = svc.sync_pending(conn)
        assert result.retryable == 1, f'Run {i+1}: expected retryable=1, got {result}'
        doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
        assert doc.state == DocumentState.OFFLINE_LOCAL_ACK, (
            f'After run {i+1}: state must still be OFFLINE_LOCAL_ACK, got {doc.state}'
        )
        assert doc.recovery_attempts == i + 1

    # Run MAX-th time: exhausted → REQUIRES_MANUAL_RECONCILIATION
    result = svc.sync_pending(conn)
    assert result.manual == 1, f'Expected manual=1 on exhaustion, got {result}'

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION, (
        f'After max retries: state must be REQUIRES_MANUAL_RECONCILIATION, got {doc.state}'
    )
    assert doc.recovery_attempts == MAX


# ---------------------------------------------------------------------------
# T-unknown — unrecognised DPS state treated as retryable
# ---------------------------------------------------------------------------

def test_tunknown_unrecognised_dps_state_treated_as_retryable(conn) -> None:
    """If DPS returns an unknown state string, must stay OFFLINE_LOCAL_ACK (retryable).

    Guards against ValueError from DocumentState() crashing inside an open
    BEGIN IMMEDIATE and leaving the connection in an unusable state.
    """
    class _UnknownStateTransport:
        def send(self, *, document_id, signed_payload, fiscal_number,
                 backend_profile_id, transport_profile_id,
                 operation_type=None, request_payload=None,
                 request_payload_json=None, external_request_id=None,
                 transport_profile=None, **kwargs):
            from prro_gateway.ports import SendResult
            return SendResult(
                state='COMPLETELY_UNKNOWN_STATUS',  # not in DocumentState enum
                transport_request_id='tr-unknown',
            )

    transport = _UnknownStateTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=7001)

    svc = OfflineSyncService(transport_client=transport, max_recovery_attempts=5)
    result = svc.sync_pending(conn)

    assert result.retryable == 1, f'Unknown state must be retryable, got {result}'
    assert result.synced == 0
    assert result.manual == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.OFFLINE_LOCAL_ACK, (
        f'Unknown DPS state must keep OFFLINE_LOCAL_ACK, got {doc.state}'
    )
    assert doc.recovery_attempts == 1

    # Connection must be usable for further queries after unknown-state handling
    count = FiscalDocumentRepository.count_pending_for_offline_sync(conn)
    assert count == 1, f'Connection must be clean after unknown-state error, count={count}'


# ---------------------------------------------------------------------------
# T3b — ACK enqueues outbox (parity with reconciliation)
# ---------------------------------------------------------------------------

def test_t3b_ack_enqueues_outbox(conn) -> None:
    """After successful ACK sync, a sync_outbox row must be created.

    This mirrors reconciliation ACK behaviour: OutboxRepository.enqueue_document
    with destination='CLOUD_HUB'.  Without this, offline-synced receipts would
    never replicate to the cloud hub.
    """
    transport = _AckTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=8001)

    svc = OfflineSyncService(transport_client=transport)
    result = svc.sync_pending(conn)

    assert result.synced == 1

    row = conn.execute(
        "SELECT outbox_id, aggregate_id, destination, status FROM sync_outbox WHERE aggregate_id = ?",
        (doc_id,),
    ).fetchone()
    assert row is not None, 'sync_outbox row must exist after ACK sync'
    assert row[0] == f'{doc_id}-offline-sync-outbox'
    assert row[2] == 'CLOUD_HUB'
    assert row[3] == 'PENDING'


# ---------------------------------------------------------------------------
# T-rejected — SendResult.state='REJECTED' → REJECTED
# ---------------------------------------------------------------------------

class _RejectedResultTransport:
    """Returns SendResult with state='REJECTED' (DPS business rejection)."""

    def __init__(self):
        self.calls: list[str] = []

    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str,
             backend_profile_id: str, transport_profile_id: str,
             operation_type: str | None = None,
             request_payload: dict | None = None,
             request_payload_json: str | None = None,
             external_request_id: str | None = None,
             transport_profile=None, **kwargs) -> SendResult:
        self.calls.append(document_id)
        return SendResult(
            state=DocumentState.REJECTED.value,
            transport_request_id=f'tr-rej-{document_id[:8]}',
            submission_status='REJECTED',
            response_json='{"error":"fiscal data invalid"}',
        )


def test_t_rejected_sendresult_state_rejected(conn) -> None:
    """SendResult.state='REJECTED' must transition to REJECTED, not async SENT.

    This is the DPS business rejection path (distinct from TransportRejectedError
    which is an HTTP-level rejection → REQUIRES_MANUAL_RECONCILIATION).
    """
    transport = _RejectedResultTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=9001)

    svc = OfflineSyncService(transport_client=transport)
    result = svc.sync_pending(conn)

    assert result.rejected == 1, f'Expected rejected=1, got {result}'
    assert result.pending_async == 0, 'REJECTED must NOT count as pending_async'
    assert result.synced == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.REJECTED, (
        f'DPS REJECTED must set state=REJECTED, got {doc.state}'
    )
    assert doc.submission_status == 'REJECTED'

    # Not in any active queue
    sync_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    assert not any(r.document_id == doc_id for r in sync_docs), (
        'REJECTED doc must not remain in offline sync queue'
    )

    # Audit event
    audit = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id = ? ORDER BY rowid DESC LIMIT 1",
        (doc_id,),
    ).fetchone()
    assert audit is not None
    assert audit[0] == 'OFFLINE_SYNC_REJECTED', f'Unexpected audit event: {audit[0]!r}'


# ---------------------------------------------------------------------------
# T-unexpected-ceiling — valid-but-unexpected state escalates after max retries
# ---------------------------------------------------------------------------

class _UnexpectedValidStateTransport:
    """Returns SendResult with state='ERROR_RETRYABLE' — valid enum but not
    in the offline sync allowlist (ACK, REJECTED, SENT, KVT1, KVT2)."""

    def send(self, *, document_id, signed_payload, fiscal_number,
             backend_profile_id, transport_profile_id,
             operation_type=None, request_payload=None,
             request_payload_json=None, external_request_id=None,
             transport_profile=None, **kwargs):
        return SendResult(
            state=DocumentState.ERROR_RETRYABLE.value,
            transport_request_id='tr-unexpected',
        )


def test_t_unexpected_valid_state_ceiling(conn) -> None:
    """Valid-but-unexpected DPS state must escalate to REQUIRES_MANUAL_RECONCILIATION
    after max_recovery_attempts, not loop forever in OFFLINE_LOCAL_ACK."""
    transport = _UnexpectedValidStateTransport()
    doc_id = _insert_doc(conn, offline_fiscal_no=10001)
    MAX = 3
    svc = OfflineSyncService(transport_client=transport, max_recovery_attempts=MAX)

    # Run MAX-1 times: stays retryable
    for i in range(MAX - 1):
        result = svc.sync_pending(conn)
        assert result.retryable == 1, f'Run {i+1}: expected retryable=1, got {result}'
        doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
        assert doc.state == DocumentState.OFFLINE_LOCAL_ACK, (
            f'After run {i+1}: must stay OFFLINE_LOCAL_ACK, got {doc.state}'
        )
        assert doc.recovery_attempts == i + 1

    # Run MAX-th time: ceiling reached → REQUIRES_MANUAL_RECONCILIATION
    result = svc.sync_pending(conn)
    assert result.manual == 1, f'Expected manual=1 on ceiling, got {result}'

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION, (
        f'After max retries: must be REQUIRES_MANUAL_RECONCILIATION, got {doc.state}'
    )
    assert doc.recovery_attempts == MAX

    # Audit trail must show terminal event
    audit = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id = ? ORDER BY rowid DESC LIMIT 1",
        (doc_id,),
    ).fetchone()
    assert audit[0] == 'OFFLINE_SYNC_FAILED_TERMINAL'


# ---------------------------------------------------------------------------
# T8 — reconciliation queue invariant after mixed sync
# ---------------------------------------------------------------------------

def test_t8_reconciliation_queue_unchanged_for_remaining_offline(conn) -> None:
    """After sync: SENT docs enter reconciliation queue; OFFLINE_LOCAL_ACK remain disjoint."""
    ack_transport = _AckTransport()
    doc_acked = _insert_doc(conn, offline_fiscal_no=6001)

    # Pre-seed an online SENT doc (simulated crash-after-send)
    sent_id = _insert_doc(
        conn,
        fs_mode='ONLINE',
        offline_fiscal_no=None,
        target_state=DocumentState.SENT,
        submission_status=None,
    )

    svc = OfflineSyncService(transport_client=ack_transport)
    result = svc.sync_pending(conn)

    assert result.synced == 1

    offline_sync_after = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    reconciliation_after = FiscalDocumentRepository.get_pending_for_reconciliation(conn)

    offline_ids = {r.document_id for r in offline_sync_after}
    recon_ids = {r.document_id for r in reconciliation_after}

    # ACKed doc not in either queue
    assert doc_acked not in offline_ids, 'ACKed doc must not remain in sync queue'
    assert doc_acked not in recon_ids, 'ACKed doc must not be in reconciliation queue'

    # Pre-existing online SENT doc only in reconciliation queue
    assert sent_id in recon_ids, 'Online SENT doc must remain in reconciliation queue'
    assert sent_id not in offline_ids, 'Online SENT doc must not be in offline sync queue'

    # The two queues remain disjoint
    assert offline_ids & recon_ids == set(), (
        f'Queues must be disjoint after sync, overlap: {offline_ids & recon_ids}'
    )
